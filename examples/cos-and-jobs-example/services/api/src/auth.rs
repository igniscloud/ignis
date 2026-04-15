use std::env;

use base64::Engine;
use ignis_sdk::http::Context;
use rand::distr::{Alphanumeric, SampleString};
use sha2::{Digest, Sha256};
use urlencoding::encode;
use wstd::http::{Body, Method, Request, Response, StatusCode};

use crate::constants::{
    CALLBACK_PATH, CLIENT_ID_ENV, CLIENT_SECRET_ENV, DEPLOYED_API_PREFIX, IGNISCLOUD_ID_BASE_URL,
    NEXT_COOKIE, SESSION_COOKIE, STATE_COOKIE, VERIFIER_COOKIE,
};
use crate::db::{ensure_schema, upsert_registered_user};
use crate::models::{
    ApiEnvelope, ExampleConfig, SimpleMessage, TokenExchangeRequest, TokenResponseData, UserInfo,
};
use crate::response::{
    clear_cookie, ephemeral_cookie, json_response_with_cookies, redirect_to_frontend_error,
    redirect_with_cookies, session_cookie,
};
use crate::util::{api_error_message, cookie_value, header_value, http_client, parse_query_map};

pub(crate) async fn handle_auth_start(context: Context) -> Response<Body> {
    let config = match read_config() {
        Ok(config) => config,
        Err(error) => return redirect_to_frontend_error("/", &error),
    };
    let redirect_uri = callback_url(&context);
    let state = random_token(24);
    let verifier = random_token(64);
    let challenge = code_challenge(&verifier);
    let next_path = requested_next_path(&context);
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
            ephemeral_cookie(NEXT_COOKIE, &next_path),
        ],
    )
}

pub(crate) async fn handle_auth_callback(context: Context) -> Response<Body> {
    let config = match read_config() {
        Ok(config) => config,
        Err(error) => return redirect_to_frontend_error("/", &error),
    };
    let query = parse_query_map(context.request().uri().query());
    let redirect_target = cookie_value(context.request().headers(), NEXT_COOKIE)
        .filter(|value| value.starts_with('/'))
        .unwrap_or_else(|| "/".to_owned());
    if let Some(error) = query.get("error") {
        return redirect_to_frontend_error(
            &redirect_target,
            query
                .get("error_description")
                .map(String::as_str)
                .unwrap_or(error),
        );
    }

    let Some(code) = query.get("code") else {
        return redirect_to_frontend_error(&redirect_target, "callback is missing `code`");
    };
    let Some(returned_state) = query.get("state") else {
        return redirect_to_frontend_error(&redirect_target, "callback is missing `state`");
    };
    let headers = context.request().headers();
    let Some(expected_state) = cookie_value(headers, STATE_COOKIE) else {
        return redirect_to_frontend_error(
            &redirect_target,
            "temporary login state cookie is missing",
        );
    };
    let Some(verifier) = cookie_value(headers, VERIFIER_COOKIE) else {
        return redirect_to_frontend_error(
            &redirect_target,
            "temporary PKCE verifier cookie is missing",
        );
    };
    if returned_state != &expected_state {
        return redirect_to_frontend_error(&redirect_target, "callback state mismatch");
    }

    let redirect_uri = callback_url(&context);
    let tokens = match exchange_authorization_code(&config, &redirect_uri, code, &verifier).await {
        Ok(tokens) => tokens,
        Err(error) => return redirect_to_frontend_error(&redirect_target, &error),
    };
    let user = match fetch_userinfo(&config, &tokens.access_token).await {
        Ok(user) => user,
        Err(error) => return redirect_to_frontend_error(&redirect_target, &error),
    };
    if let Err(error) = ensure_schema().and_then(|_| upsert_registered_user(&user)) {
        return redirect_to_frontend_error(&redirect_target, &error);
    }

    redirect_with_cookies(
        StatusCode::SEE_OTHER,
        &redirect_target,
        &[
            session_cookie(&tokens.access_token, tokens.expires_in),
            clear_cookie(STATE_COOKIE),
            clear_cookie(VERIFIER_COOKIE),
            clear_cookie(NEXT_COOKIE),
        ],
    )
}

pub(crate) fn handle_logout() -> Response<Body> {
    json_response_with_cookies(
        StatusCode::OK,
        SimpleMessage {
            ok: true,
            message: "Signed out".to_owned(),
        },
        &[
            clear_cookie(SESSION_COOKIE),
            clear_cookie(STATE_COOKIE),
            clear_cookie(VERIFIER_COOKIE),
            clear_cookie(NEXT_COOKIE),
        ],
    )
}

pub(crate) async fn current_session_user(
    context: &Context,
) -> std::result::Result<Option<UserInfo>, String> {
    let config = read_config()?;
    let Some(access_token) = cookie_value(context.request().headers(), SESSION_COOKIE) else {
        return Ok(None);
    };
    fetch_userinfo(&config, &access_token).await.map(Some)
}

pub(crate) async fn require_authenticated_user(
    context: &Context,
) -> std::result::Result<UserInfo, String> {
    current_session_user(context)
        .await?
        .ok_or_else(|| "login required".to_owned())
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
        return Err(api_error_message(
            "userinfo request failed",
            &payload,
            status,
        ));
    }
    let envelope: ApiEnvelope<UserInfo> = serde_json::from_str(&payload)
        .map_err(|error| format!("parsing userinfo response failed: {error}"))?;
    Ok(envelope.data)
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

fn requested_next_path(context: &Context) -> String {
    parse_query_map(context.request().uri().query())
        .get("next")
        .filter(|value| value.starts_with('/'))
        .cloned()
        .unwrap_or_else(|| "/".to_owned())
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

fn random_token(length: usize) -> String {
    Alphanumeric.sample_string(&mut rand::rng(), length)
}

fn code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}
