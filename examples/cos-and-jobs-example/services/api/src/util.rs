use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use ignis_sdk::http::Context;
use serde::Deserialize;
use urlencoding::decode;
use wstd::http::{Client, StatusCode};
use wstd::time::Duration;

use crate::models::ApiErrorEnvelope;

pub(crate) async fn read_json_body<T: for<'de> Deserialize<'de>>(
    context: Context,
) -> std::result::Result<T, String> {
    let mut request = context.into_request();
    let body = request
        .body_mut()
        .str_contents()
        .await
        .map_err(|error| format!("reading request body failed: {error}"))?
        .to_owned();
    serde_json::from_str(&body).map_err(|error| format!("invalid JSON body: {error}"))
}

pub(crate) fn http_client() -> Client {
    let mut client = Client::new();
    client.set_connect_timeout(Duration::from_secs(5));
    client.set_first_byte_timeout(Duration::from_secs(10));
    client.set_between_bytes_timeout(Duration::from_secs(10));
    client
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis() as i64
}

pub(crate) fn header_value(headers: &wstd::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn cookie_value(headers: &wstd::http::HeaderMap, name: &str) -> Option<String> {
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

pub(crate) fn parse_query_map(query: Option<&str>) -> BTreeMap<String, String> {
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

pub(crate) fn api_error_message(prefix: &str, payload: &str, status: StatusCode) -> String {
    match serde_json::from_str::<ApiErrorEnvelope>(payload) {
        Ok(envelope) => format!("{prefix} ({status}): {}", envelope.error),
        Err(_) => format!("{prefix} ({status}): {}", payload.trim()),
    }
}
