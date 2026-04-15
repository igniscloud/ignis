use serde::Serialize;
use urlencoding::encode;
use wstd::http::{Body, Response, StatusCode};

use crate::constants::SESSION_COOKIE;
use crate::models::JsonError;

pub(crate) fn json_response<T: Serialize>(status: StatusCode, payload: T) -> Response<Body> {
    let body = serde_json::to_string(&payload).expect("serialize json response");
    Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=utf-8")
        .body(Body::from(body))
        .expect("json response")
}

pub(crate) fn json_response_with_cookies<T: Serialize>(
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

pub(crate) fn json_error(status: StatusCode, message: &str) -> Response<Body> {
    json_response(status, JsonError { error: message })
}

pub(crate) fn redirect_with_cookies(
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

pub(crate) fn redirect_to_frontend_error(path: &str, message: &str) -> Response<Body> {
    let location = format!("{path}?error={}", encode(message));
    redirect_with_cookies(
        StatusCode::SEE_OTHER,
        &location,
        &[
            clear_cookie(crate::constants::STATE_COOKIE),
            clear_cookie(crate::constants::VERIFIER_COOKIE),
            clear_cookie(crate::constants::NEXT_COOKIE),
        ],
    )
}

pub(crate) fn session_cookie(access_token: &str, expires_in: u64) -> String {
    format!(
        "{SESSION_COOKIE}={access_token}; Path=/; Max-Age={expires_in}; HttpOnly; Secure; SameSite=Lax"
    )
}

pub(crate) fn ephemeral_cookie(name: &str, value: &str) -> String {
    format!("{name}={value}; Path=/; Max-Age=600; HttpOnly; Secure; SameSite=Lax")
}

pub(crate) fn clear_cookie(name: &str) -> String {
    format!("{name}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax")
}
