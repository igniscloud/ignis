use std::collections::BTreeMap;

use ignis_sdk::object_store::{self, PresignedUrl};
use serde::{Deserialize, Serialize};

use crate::constants::USER_LIMIT_BYTES;

pub(crate) struct ExampleConfig {
    pub(crate) igniscloud_id_base_url: String,
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
}

#[derive(Serialize)]
pub(crate) struct ApiRootPayload {
    pub(crate) name: &'static str,
    pub(crate) limit_bytes: u64,
    pub(crate) endpoints: Vec<&'static str>,
}

#[derive(Serialize)]
pub(crate) struct SimpleMessage {
    pub(crate) ok: bool,
    pub(crate) message: String,
}

#[derive(Serialize)]
pub(crate) struct SessionPayload {
    authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    nickname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<String>,
    limit_bytes: u64,
    used_bytes: u64,
    remaining_bytes: u64,
    message: String,
}

impl SessionPayload {
    pub(crate) fn signed_out(message: String) -> Self {
        Self {
            authenticated: false,
            nickname: None,
            avatar_url: None,
            subject: None,
            limit_bytes: USER_LIMIT_BYTES,
            used_bytes: 0,
            remaining_bytes: USER_LIMIT_BYTES,
            message,
        }
    }

    pub(crate) fn signed_in(user: UserInfo, used_bytes: u64) -> Self {
        Self {
            authenticated: true,
            nickname: Some(user.display_nickname()),
            avatar_url: user.avatar_url,
            subject: Some(user.sub),
            limit_bytes: USER_LIMIT_BYTES,
            used_bytes,
            remaining_bytes: USER_LIMIT_BYTES.saturating_sub(used_bytes),
            message: "Signed in".to_owned(),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct CreateUploadRequest {
    pub(crate) filename: String,
    pub(crate) content_type: String,
    pub(crate) size_bytes: u64,
}

#[derive(Deserialize)]
pub(crate) struct CompleteUploadRequest {
    pub(crate) file_id: String,
}

#[derive(Serialize)]
pub(crate) struct UploadListPayload {
    pub(crate) limit_bytes: u64,
    pub(crate) used_bytes: u64,
    pub(crate) remaining_bytes: u64,
    pub(crate) uploads: Vec<UploadRecord>,
}

#[derive(Serialize)]
pub(crate) struct PresignUploadPayload {
    pub(crate) file_id: String,
    pub(crate) upload_url: String,
    pub(crate) method: String,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) expires_at_ms: Option<u64>,
    pub(crate) limit_bytes: u64,
    pub(crate) used_bytes_after: u64,
    pub(crate) remaining_bytes_after: u64,
}

#[derive(Serialize)]
pub(crate) struct DownloadPayload {
    file_id: String,
    download_url: String,
    method: String,
    headers: BTreeMap<String, String>,
    expires_at_ms: Option<u64>,
}

impl DownloadPayload {
    pub(crate) fn from_presigned(url: PresignedUrl) -> Self {
        Self {
            file_id: url.file_id,
            download_url: url.url,
            method: url.method,
            headers: headers_to_map(url.headers),
            expires_at_ms: url.expires_at_ms,
        }
    }
}

pub(crate) struct JobRequestContext {
    pub(crate) job_id: String,
    pub(crate) trigger: String,
}

#[derive(Serialize)]
pub(crate) struct CleanupPendingUploadsPayload {
    pub(crate) ok: bool,
    pub(crate) job_id: String,
    pub(crate) trigger: String,
    pub(crate) cutoff_ms: i64,
    pub(crate) older_than_ms: i64,
    pub(crate) expired_uploads: u64,
    pub(crate) deleted_uploads: u64,
    pub(crate) released_bytes: u64,
}

#[derive(Serialize)]
pub(crate) struct UploadRecord {
    pub(crate) file_id: String,
    pub(crate) filename: String,
    pub(crate) content_type: String,
    pub(crate) size_bytes: u64,
    pub(crate) status: String,
    pub(crate) created_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) completed_at_ms: Option<i64>,
}

#[derive(Serialize)]
pub(crate) struct LimitErrorPayload {
    pub(crate) error: &'static str,
    pub(crate) limit_bytes: u64,
    pub(crate) used_bytes: u64,
    pub(crate) remaining_bytes: u64,
    pub(crate) requested_bytes: u64,
}

#[derive(Serialize)]
pub(crate) struct JsonError<'a> {
    pub(crate) error: &'a str,
}

#[derive(Serialize)]
pub(crate) struct TokenExchangeRequest<'a> {
    pub(crate) grant_type: &'a str,
    pub(crate) client_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) client_secret: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) redirect_uri: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) code_verifier: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) refresh_token: Option<&'a str>,
}

#[derive(Deserialize)]
pub(crate) struct ApiEnvelope<T> {
    pub(crate) data: T,
}

#[derive(Deserialize)]
pub(crate) struct ApiErrorEnvelope {
    pub(crate) error: String,
}

#[derive(Deserialize)]
pub(crate) struct TokenResponseData {
    pub(crate) access_token: String,
    pub(crate) expires_in: u64,
}

#[derive(Clone, Deserialize)]
pub(crate) struct UserInfo {
    pub(crate) sub: String,
    #[serde(default)]
    pub(crate) display_name: Option<String>,
    #[serde(default)]
    pub(crate) avatar_url: Option<String>,
}

impl UserInfo {
    pub(crate) fn display_nickname(&self) -> String {
        self.display_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| self.sub.clone())
    }
}

pub(crate) fn headers_to_map(headers: Vec<object_store::Header>) -> BTreeMap<String, String> {
    headers
        .into_iter()
        .map(|header| (header.name, header.value))
        .collect()
}
