use std::collections::BTreeMap;
use std::env;

use base64::Engine;
use ignis_sdk::http::{Context, Router};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use urlencoding::{decode, encode};
use wstd::http::{Body, Client, Method, Request, Response, Result, StatusCode};
use wstd::time::Duration;

const CLIENT_ID_ENV: &str = "IGNIS_LOGIN_CLIENT_ID";
const CLIENT_SECRET_ENV: &str = "IGNIS_LOGIN_CLIENT_SECRET";
const IGNISCLOUD_ID_BASE_URL: &str = "https://id.igniscloud.transairobot.com";
const DEPLOYED_API_PREFIX: &str = "/api";
const CALLBACK_PATH: &str = "/auth/callback";
const SESSION_COOKIE: &str = "ignis_login_session";
const STATE_COOKIE: &str = "ignis_login_state";
const VERIFIER_COOKIE: &str = "ignis_login_verifier";

#[wstd::http_server]
async fn main(req: Request<Body>) -> Result<Response<Body>> {
    let router = build_router();
    Ok(router.handle(req).await)
}

fn build_router() -> Router {
    let mut router = Router::new();

    router
        .get("/", |_context: Context| async move { handle_root() })
        .expect("register GET /");

    router
        .get("/me", |context: Context| async move { handle_me(context).await })
        .expect("register GET /me");

    router
        .get("/auth/start", |context: Context| async move {
            handle_auth_start(context).await
        })
        .expect("register GET /auth/start");

    router
        .get("/auth/callback", |context: Context| async move {
            handle_auth_callback(context).await
        })
        .expect("register GET /auth/callback");

    router
        .route(Method::POST, "/logout", |context: Context| async move {
            handle_logout(context)
        })
        .expect("register POST /logout");

    router
}

fn handle_root() -> Response<Body> {
    json_response(
        StatusCode::OK,
        ApiRootPayload {
            name: "ignis-login-example-api",
            endpoints: vec![
                "GET /me",
                "GET /auth/start",
                "GET /auth/callback",
                "POST /logout",
            ],
        },
    )
}

async fn handle_me(context: Context) -> Response<Body> {
    let config = match read_config() {
        Ok(config) => config,
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error),
    };
    let Some(access_token) = cookie_value(context.request().headers(), SESSION_COOKIE) else {
        return json_response(
            StatusCode::OK,
            SessionPayload::signed_out("No active login session".to_owned()),
        );
    };

    match fetch_userinfo(&config, &access_token).await {
        Ok(user) => json_response(StatusCode::OK, SessionPayload::signed_in(user)),
        Err(error) => json_response_with_cookies(
            StatusCode::OK,
            SessionPayload::signed_out(format!("Session expired or invalid: {error}")),
            &[clear_cookie(SESSION_COOKIE)],
        ),
    }
}

async fn handle_auth_start(context: Context) -> Response<Body> {
    let config = match read_config() {
        Ok(config) => config,
        Err(error) => return redirect_to_frontend_error(&error),
    };
    let redirect_uri = callback_url(&context);
    let state = random_token(24);
    let verifier = random_token(64);
    let challenge = code_challenge(&verifier);
    let login_url = format!(
        "{}?client_id={}&redirect_uri={}&state={}&code_challenge={}&code_challenge_method=S256",
        hosted_login_url(&config.igniscloud_id_base_url),
        encode(&config.client_id),
        encode(&redirect_uri),
        encode(&state),
        encode(&challenge),
    );

    redirect_with_cookies(
        StatusCode::SEE_OTHER,
        &login_url,
        &[
            ephemeral_cookie(STATE_COOKIE, &state),
            ephemeral_cookie(VERIFIER_COOKIE, &verifier),
        ],
    )
}

async fn handle_auth_callback(context: Context) -> Response<Body> {
    let config = match read_config() {
        Ok(config) => config,
        Err(error) => return redirect_to_frontend_error(&error),
    };
    let query = parse_query_map(context.request().uri().query());
    if let Some(error) = query.get("error") {
        return redirect_to_frontend_error(
            query
                .get("error_description")
                .map(String::as_str)
                .unwrap_or(error),
        );
    }

    let Some(code) = query.get("code") else {
        return redirect_to_frontend_error("callback is missing `code`");
    };
    let Some(returned_state) = query.get("state") else {
        return redirect_to_frontend_error("callback is missing `state`");
    };

    let headers = context.request().headers();
    let Some(expected_state) = cookie_value(headers, STATE_COOKIE) else {
        return redirect_to_frontend_error("temporary login state cookie is missing");
    };
    let Some(verifier) = cookie_value(headers, VERIFIER_COOKIE) else {
        return redirect_to_frontend_error("temporary PKCE verifier cookie is missing");
    };
    if returned_state != &expected_state {
        return redirect_to_frontend_error("callback `state` does not match the stored login state");
    }

    let redirect_uri = callback_url(&context);
    let tokens = match exchange_authorization_code(&config, &redirect_uri, code, &verifier).await {
        Ok(tokens) => tokens,
        Err(error) => return redirect_to_frontend_error(&error),
    };

    redirect_with_cookies(
        StatusCode::SEE_OTHER,
        "/",
        &[
            session_cookie(&tokens.access_token, tokens.expires_in),
            clear_cookie(STATE_COOKIE),
            clear_cookie(VERIFIER_COOKIE),
        ],
    )
}

fn handle_logout(_context: Context) -> Response<Body> {
    json_response_with_cookies(
        StatusCode::OK,
        SimpleMessage {
            ok: true,
            message: "Signed out".to_owned(),
        },
        &[clear_cookie(SESSION_COOKIE)],
    )
}

async fn exchange_authorization_code(
    config: &ExampleConfig,
    redirect_uri: &str,
    code: &str,
    verifier: &str,
) -> std::result::Result<TokenResponseData, String> {
    let body = serde_json::to_string(&TokenExchangeRequest {
        grant_type: "authorization_code",
        client_id: &config.client_id,
        client_secret: Some(&config.client_secret),
        code: Some(code),
        redirect_uri: Some(redirect_uri),
        code_verifier: Some(verifier),
        refresh_token: None,
    })
    .map_err(|error| format!("serializing token request failed: {error}"))?;

    let request = Request::builder()
        .method(Method::POST)
        .uri(token_url(&config.igniscloud_id_base_url))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .map_err(|error| format!("building token request failed: {error}"))?;

    let mut response = http_client()
        .send(request)
        .await
        .map_err(|error| format!("calling /oauth2/token failed: {error}"))?;
    let status = response.status();
    let payload = response
        .body_mut()
        .str_contents()
        .await
        .map_err(|error| format!("reading token response failed: {error}"))?
        .to_owned();

    if !status.is_success() {
        return Err(api_error_message("token exchange failed", &payload, status));
    }

    let envelope: ApiEnvelope<TokenResponseData> = serde_json::from_str(&payload)
        .map_err(|error| format!("parsing token response failed: {error}"))?;
    Ok(envelope.data)
}

async fn fetch_userinfo(
    config: &ExampleConfig,
    access_token: &str,
) -> std::result::Result<UserInfo, String> {
    let request = Request::builder()
        .method(Method::GET)
        .uri(userinfo_url(&config.igniscloud_id_base_url))
        .header("authorization", format!("Bearer {access_token}"))
        .body(Body::empty())
        .map_err(|error| format!("building userinfo request failed: {error}"))?;

    let mut response = http_client()
        .send(request)
        .await
        .map_err(|error| format!("calling /oidc/userinfo failed: {error}"))?;
    let status = response.status();
    let payload = response
        .body_mut()
        .str_contents()
        .await
        .map_err(|error| format!("reading userinfo response failed: {error}"))?
        .to_owned();

    if !status.is_success() {
        return Err(api_error_message("userinfo request failed", &payload, status));
    }

    let envelope: ApiEnvelope<UserInfo> = serde_json::from_str(&payload)
        .map_err(|error| format!("parsing userinfo response failed: {error}"))?;
    Ok(envelope.data)
}

fn http_client() -> Client {
    let mut client = Client::new();
    client.set_connect_timeout(Duration::from_secs(5));
    client.set_first_byte_timeout(Duration::from_secs(10));
    client.set_between_bytes_timeout(Duration::from_secs(10));
    client
}

struct ExampleConfig {
    igniscloud_id_base_url: String,
    client_id: String,
    client_secret: String,
}

#[derive(Serialize)]
struct ApiRootPayload {
    name: &'static str,
    endpoints: Vec<&'static str>,
}

#[derive(Serialize)]
struct SimpleMessage {
    ok: bool,
    message: String,
}

#[derive(Serialize)]
struct SessionPayload {
    authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    nickname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<String>,
    message: String,
}

impl SessionPayload {
    fn signed_out(message: String) -> Self {
        Self {
            authenticated: false,
            nickname: None,
            avatar_url: None,
            subject: None,
            message,
        }
    }

    fn signed_in(user: UserInfo) -> Self {
        Self {
            authenticated: true,
            nickname: Some(display_nickname(&user)),
            avatar_url: user.avatar_url,
            subject: Some(user.sub),
            message: "Signed in".to_owned(),
        }
    }
}

#[derive(Serialize)]
struct JsonError<'a> {
    error: &'a str,
}

#[derive(Serialize)]
struct TokenExchangeRequest<'a> {
    grant_type: &'a str,
    client_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_secret: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_uri: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code_verifier: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_token: Option<&'a str>,
}

#[derive(Deserialize)]
struct ApiEnvelope<T> {
    data: T,
}

#[derive(Deserialize)]
struct ApiErrorEnvelope {
    error: String,
}

#[derive(Deserialize)]
struct TokenResponseData {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct UserInfo {
    sub: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    avatar_url: Option<String>,
}

fn read_config() -> std::result::Result<ExampleConfig, String> {
    Ok(ExampleConfig {
        igniscloud_id_base_url: IGNISCLOUD_ID_BASE_URL.to_owned(),
        client_id: required_env(CLIENT_ID_ENV)?,
        client_secret: required_env(CLIENT_SECRET_ENV)?,
    })
}

fn required_env(name: &str) -> std::result::Result<String, String> {
    let value = env::var(name).map_err(|_| format!("missing env var `{name}`"))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("env var `{name}` cannot be empty"));
    }
    Ok(trimmed.to_owned())
}

fn request_origin(context: &Context) -> String {
    let headers = context.request().headers();
    let host = header_value(headers, "x-forwarded-host")
        .or_else(|| header_value(headers, "host"))
        .unwrap_or_else(|| "localhost".to_owned());
    let proto = header_value(headers, "x-forwarded-proto").unwrap_or_else(|| {
        if host.starts_with("127.0.0.1") || host.starts_with("localhost") {
            "http".to_owned()
        } else {
            "https".to_owned()
        }
    });
    format!("{proto}://{host}")
}

fn callback_url(context: &Context) -> String {
    format!(
        "{}{}{}",
        request_origin(context),
        deployed_api_prefix(context),
        CALLBACK_PATH
    )
}

fn deployed_api_prefix(context: &Context) -> &'static str {
    let host = header_value(context.request().headers(), "x-forwarded-host")
        .or_else(|| header_value(context.request().headers(), "host"))
        .unwrap_or_default();
    if host.starts_with("127.0.0.1") || host.starts_with("localhost") {
        ""
    } else {
        DEPLOYED_API_PREFIX
    }
}

fn header_value(headers: &wstd::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn cookie_value(headers: &wstd::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| {
            raw.split(';').find_map(|part| {
                let (cookie_name, cookie_value) = part.trim().split_once('=')?;
                if cookie_name == name {
                    Some(cookie_value.to_owned())
                } else {
                    None
                }
            })
        })
}

fn hosted_login_url(base_url: &str) -> String {
    format!("{}/login", base_url.trim_end_matches('/'))
}

fn token_url(base_url: &str) -> String {
    format!("{}/oauth2/token", base_url.trim_end_matches('/'))
}

fn userinfo_url(base_url: &str) -> String {
    format!("{}/oidc/userinfo", base_url.trim_end_matches('/'))
}

fn parse_query_map(query: Option<&str>) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let Some(query) = query else {
        return map;
    };
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (raw_key, raw_value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = decode(raw_key)
            .map(|value| value.into_owned())
            .unwrap_or_else(|_| raw_key.to_owned());
        let value = decode(raw_value)
            .map(|value| value.into_owned())
            .unwrap_or_else(|_| raw_value.to_owned());
        map.insert(key, value);
    }
    map
}

fn random_token(length: usize) -> String {
    Alphanumeric.sample_string(&mut rand::rng(), length)
}

fn code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn session_cookie(access_token: &str, expires_in: u64) -> String {
    format!(
        "{SESSION_COOKIE}={access_token}; Path=/; Max-Age={expires_in}; HttpOnly; Secure; SameSite=Lax"
    )
}

fn ephemeral_cookie(name: &str, value: &str) -> String {
    format!("{name}={value}; Path=/; Max-Age=600; HttpOnly; Secure; SameSite=Lax")
}

fn clear_cookie(name: &str) -> String {
    format!("{name}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax")
}

fn redirect_with_cookies(
    status: StatusCode,
    location: &str,
    cookies: &[String],
) -> Response<Body> {
    let mut response = Response::builder()
        .status(status)
        .header("location", location)
        .body(Body::empty())
        .expect("redirect response");
    for cookie in cookies {
        response
            .headers_mut()
            .append("set-cookie", cookie.parse().expect("valid set-cookie"));
    }
    response
}

fn redirect_to_frontend_error(message: &str) -> Response<Body> {
    let location = format!("/?error={}", encode(message));
    redirect_with_cookies(
        StatusCode::SEE_OTHER,
        &location,
        &[clear_cookie(STATE_COOKIE), clear_cookie(VERIFIER_COOKIE)],
    )
}

fn json_response<T: Serialize>(status: StatusCode, payload: T) -> Response<Body> {
    let body = serde_json::to_string(&payload).expect("serialize json response");
    Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=utf-8")
        .body(Body::from(body))
        .expect("json response")
}

fn json_response_with_cookies<T: Serialize>(
    status: StatusCode,
    payload: T,
    cookies: &[String],
) -> Response<Body> {
    let mut response = json_response(status, payload);
    for cookie in cookies {
        response
            .headers_mut()
            .append("set-cookie", cookie.parse().expect("valid set-cookie"));
    }
    response
}

fn json_error(status: StatusCode, message: &str) -> Response<Body> {
    json_response(status, JsonError { error: message })
}

fn display_nickname(user: &UserInfo) -> String {
    user.display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| user.sub.clone())
}

fn api_error_message(prefix: &str, payload: &str, status: StatusCode) -> String {
    match serde_json::from_str::<ApiErrorEnvelope>(payload) {
        Ok(envelope) => format!("{prefix} ({status}): {}", envelope.error),
        Err(_) => format!("{prefix} ({status}): {}", payload.trim()),
    }
}
