use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::api::ApiClient;
use crate::config;
use crate::output::{self, Warning};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
const LOGIN_SUCCESS_HTML: &str = "<!doctype html><html><body><h1>Login successful</h1><p>You can close this window and return to Ignis CLI.</p><script>window.close();</script></body></html>";
const LOGIN_FAILURE_HTML: &str = "<h1>Login Failed</h1><p>Return to the terminal and retry.</p>";

#[derive(Debug)]
struct LoopbackLoginPayload {
    token: String,
    user_sub: Option<String>,
    user_aud: Option<String>,
    user_display_name: Option<String>,
}

#[derive(Debug)]
struct LoopbackCallbackState {
    expected_state: String,
    sender: Mutex<Option<oneshot::Sender<Result<LoopbackLoginPayload>>>>,
}

#[derive(Debug, Serialize)]
struct LoginResult {
    region: config::Region,
    server: String,
    saved_config_path: std::path::PathBuf,
    user_sub: Option<String>,
    user_aud: Option<String>,
    user_display_name: Option<String>,
}

pub async fn login(token: Option<String>, region: Option<config::Region>) -> Result<()> {
    if token.is_some() {
        bail!("`ignis login` now uses browser sign-in; do not pass `--token`");
    }

    let region = select_login_region(region)?;
    let state = new_login_state();
    let (redirect_uri, receiver, handle) = start_loopback_login_listener(state.clone()).await?;
    let login_url = build_browser_login_url(region, &redirect_uri, &state)?;

    eprintln!(
        "Opening browser for igniscloud {} login...",
        region.as_str()
    );
    eprintln!("Login URL:\n{login_url}");
    if !open_browser(&login_url) {
        eprintln!("Browser launch failed. Open this URL in your browser.");
    }

    let wait_result = tokio::time::timeout(LOGIN_TIMEOUT, receiver).await;
    if wait_result.is_err() {
        handle.abort();
    }
    let payload_result = match wait_result {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(anyhow!(
            "loopback login listener exited before completing browser login"
        )),
        Err(_) => Err(anyhow!(
            "timed out waiting for browser login after {} seconds",
            LOGIN_TIMEOUT.as_secs()
        )),
    };

    handle
        .await
        .context("loopback login listener task panicked")?;
    let payload = payload_result?;

    let mut config = config::CliConfig::load()?.unwrap_or(config::CliConfig {
        region,
        server: region.server().to_owned(),
        token: String::new(),
        user_sub: None,
        user_aud: None,
        user_display_name: None,
        accounts: Default::default(),
    });
    config.set_account(
        region,
        payload.token,
        payload.user_sub,
        payload.user_aud,
        payload.user_display_name,
    );
    let path = config.save()?;

    output::success(LoginResult {
        region: config.region,
        server: config.server,
        saved_config_path: path,
        user_sub: config.user_sub,
        user_aud: config.user_aud,
        user_display_name: config.user_display_name,
    })
}

pub fn logout() -> Result<()> {
    match config::CliConfig::clear()? {
        Some(path) => output::success(serde_json::json!({
            "removed": true,
            "config_path": path,
        })),
        None => output::success_with(
            serde_json::json!({
                "removed": false,
                "config_path": config::default_config_path(),
            }),
            vec![Warning::new(
                "no_saved_login",
                format!(
                    "no saved login found at {}",
                    config::default_config_path().display()
                ),
            )],
            Vec::new(),
        ),
    }
}

pub async fn whoami(token: Option<String>) -> Result<()> {
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.whoami().await?;
    output::success(response)
}

fn select_login_region(region: Option<config::Region>) -> Result<config::Region> {
    if let Some(region) = region {
        return Ok(region);
    }
    if let Ok(value) = std::env::var("IGNIS_REGION") {
        let value = value.trim();
        if !value.is_empty() {
            return config::Region::parse(value);
        }
    }
    eprintln!("Select Ignis region:");
    eprintln!("  1) cn     https://api.transairobot.com/api");
    eprintln!("  2) global https://igniscloud.dev/api");
    eprint!("Region [cn]: ");
    let _ = std::io::Write::flush(&mut std::io::stderr());
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("reading region from stdin failed")?;
    let value = input.trim();
    if value.is_empty() || value == "1" {
        Ok(config::Region::Cn)
    } else if value == "2" {
        Ok(config::Region::Global)
    } else {
        config::Region::parse(value)
    }
}

fn build_browser_login_url(
    region: config::Region,
    redirect_uri: &str,
    state: &str,
) -> Result<String> {
    let mut url = reqwest::Url::parse(&format!(
        "{}/v1/cli/auth/start",
        region.server().trim_end_matches('/')
    ))?;
    url.query_pairs_mut()
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", state);
    Ok(url.to_string())
}

async fn start_loopback_login_listener(
    expected_state: String,
) -> Result<(
    String,
    oneshot::Receiver<Result<LoopbackLoginPayload>>,
    tokio::task::JoinHandle<()>,
)> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .context("binding localhost callback server failed")?;
    let port = listener
        .local_addr()
        .context("reading localhost callback address failed")?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let (sender, receiver) = oneshot::channel();

    let state = Arc::new(LoopbackCallbackState {
        expected_state,
        sender: Mutex::new(Some(sender)),
    });
    let service_state = state.clone();
    let error_state = state.clone();

    let handle = tokio::spawn(async move {
        let result = async {
            let (stream, _) = listener
                .accept()
                .await
                .context("accepting localhost callback failed")?;

            http1::Builder::new()
                .keep_alive(false)
                .serve_connection(
                    TokioIo::new(stream),
                    service_fn(move |request| {
                        let state = service_state.clone();
                        async move {
                            Ok::<_, Infallible>(handle_loopback_login_request(request, state).await)
                        }
                    }),
                )
                .await
                .context("serving localhost callback failed")
        }
        .await;

        if let Err(error) = result {
            error_state.send(Err(error));
        }
    });

    Ok((redirect_uri, receiver, handle))
}

async fn handle_loopback_login_request(
    request: Request<Incoming>,
    state: Arc<LoopbackCallbackState>,
) -> Response<Full<Bytes>> {
    let (response, result) = process_loopback_login_request(request, &state.expected_state).await;
    state.send(result);
    response
}

async fn process_loopback_login_request(
    request: Request<Incoming>,
    expected_state: &str,
) -> (Response<Full<Bytes>>, Result<LoopbackLoginPayload>) {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let query = request.uri().query().map(str::to_owned);

    if path != "/callback" {
        return failure_response(
            StatusCode::NOT_FOUND,
            "<h1>Not Found</h1><p>Unknown Ignis CLI callback path.</p>",
            anyhow!("unexpected callback path `{path}`"),
        );
    }

    let form = match method {
        Method::GET => match parse_query_values(query.as_deref()) {
            Ok(values) => values,
            Err(error) => {
                return failure_response(StatusCode::BAD_REQUEST, LOGIN_FAILURE_HTML, error);
            }
        },
        Method::POST => match parse_form_body(request.into_body()).await {
            Ok(values) => values,
            Err(error) => {
                return failure_response(StatusCode::BAD_REQUEST, LOGIN_FAILURE_HTML, error);
            }
        },
        _ => {
            return failure_response(
                StatusCode::METHOD_NOT_ALLOWED,
                "<h1>Method Not Allowed</h1><p>Ignis CLI expects a browser redirect to localhost.</p>",
                anyhow!("unexpected callback method `{method}`"),
            );
        }
    };

    let state = match form.get("state") {
        Some(state) => state,
        None => {
            return failure_response(
                StatusCode::BAD_REQUEST,
                LOGIN_FAILURE_HTML,
                anyhow!("login callback is missing state"),
            );
        }
    };
    if state != expected_state {
        return failure_response(
            StatusCode::BAD_REQUEST,
            "<h1>Login Failed</h1><p>State verification failed. Return to the terminal and retry.</p>",
            anyhow!("login callback state mismatch"),
        );
    }

    let token = match form
        .get("token")
        .cloned()
        .filter(|value| !value.trim().is_empty())
    {
        Some(token) => token,
        None => {
            return failure_response(
                StatusCode::BAD_REQUEST,
                LOGIN_FAILURE_HTML,
                anyhow!("login callback is missing token"),
            );
        }
    };

    (
        html_response(StatusCode::OK, LOGIN_SUCCESS_HTML),
        Ok(LoopbackLoginPayload {
            token,
            user_sub: form
                .get("user_sub")
                .cloned()
                .filter(|value| !value.is_empty()),
            user_aud: form
                .get("user_aud")
                .cloned()
                .filter(|value| !value.is_empty()),
            user_display_name: form
                .get("user_display_name")
                .cloned()
                .filter(|value| !value.is_empty()),
        }),
    )
}

fn parse_query_values(query: Option<&str>) -> Result<BTreeMap<String, String>> {
    match query {
        Some(query) if !query.is_empty() => {
            serde_urlencoded::from_str(query).context("parsing callback query string")
        }
        _ => Ok(BTreeMap::new()),
    }
}

async fn parse_form_body(body: Incoming) -> Result<BTreeMap<String, String>> {
    let bytes = body
        .collect()
        .await
        .context("reading localhost callback body failed")?
        .to_bytes();
    if bytes.is_empty() {
        return Ok(BTreeMap::new());
    }
    serde_urlencoded::from_bytes(&bytes).context("parsing callback form body")
}

fn failure_response(
    status: StatusCode,
    body: &str,
    error: anyhow::Error,
) -> (Response<Full<Bytes>>, Result<LoopbackLoginPayload>) {
    (html_response(status, body), Err(error))
}

fn html_response(status: StatusCode, body: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", "text/html; charset=utf-8")
        .body(Full::new(Bytes::from(body.to_owned())))
        .expect("response builder succeeds")
}

fn new_login_state() -> String {
    format!(
        "ignis-login-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default()
    )
}

fn open_browser(url: &str) -> bool {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).status()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .status()
    } else {
        std::process::Command::new("xdg-open").arg(url).status()
    };
    result.map(|status| status.success()).unwrap_or(false)
}

impl LoopbackCallbackState {
    fn send(&self, result: Result<LoopbackLoginPayload>) {
        if let Some(sender) = self.sender.lock().expect("loopback sender mutex").take() {
            let _ = sender.send(result);
        }
    }
}
