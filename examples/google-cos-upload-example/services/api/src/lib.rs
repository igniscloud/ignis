use std::collections::BTreeMap;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use ignis_sdk::http::{Context, Router};
use ignis_sdk::object_store::{self, PresignedUrl};
use ignis_sdk::sqlite::{self, SqliteValue};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use urlencoding::{decode, encode};
use wstd::http::{Body, Client, Method, Request, Response, Result, StatusCode};
use wstd::time::Duration;

const CLIENT_ID_ENV: &str = "IGNIS_LOGIN_CLIENT_ID";
const CLIENT_SECRET_ENV: &str = "IGNIS_LOGIN_CLIENT_SECRET";
const IGNISCLOUD_ID_BASE_URL: &str = "https://id.igniscloud.dev";
const DEPLOYED_API_PREFIX: &str = "/api";
const CALLBACK_PATH: &str = "/auth/callback";
const SESSION_COOKIE: &str = "google_cos_upload_session";
const STATE_COOKIE: &str = "google_cos_upload_state";
const VERIFIER_COOKIE: &str = "google_cos_upload_verifier";
const NEXT_COOKIE: &str = "google_cos_upload_next";
const USER_LIMIT_BYTES: u64 = 10 * 1024 * 1024;
const PRESIGN_EXPIRES_IN_MS: u64 = 15 * 60 * 1000;

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
        .get(
            "/me",
            |context: Context| async move { handle_me(context).await },
        )
        .expect("register GET /me");
    router
        .get("/uploads", |context: Context| async move {
            handle_list_uploads(context).await
        })
        .expect("register GET /uploads");
    router
        .route(
            Method::POST,
            "/uploads/presign",
            |context: Context| async move { handle_create_upload(context).await },
        )
        .expect("register POST /uploads/presign");
    router
        .route(
            Method::POST,
            "/uploads/complete",
            |context: Context| async move { handle_complete_upload(context).await },
        )
        .expect("register POST /uploads/complete");
    router
        .get(
            "/uploads/:file_id/download",
            |context: Context| async move { handle_download_upload(context).await },
        )
        .expect("register GET /uploads/:file_id/download");
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
        .route(Method::POST, "/logout", |_context: Context| async move {
            handle_logout()
        })
        .expect("register POST /logout");

    router
}

fn handle_root() -> Response<Body> {
    json_response(
        StatusCode::OK,
        ApiRootPayload {
            name: "google-cos-upload-example-api",
            limit_bytes: USER_LIMIT_BYTES,
            endpoints: vec![
                "GET /me",
                "GET /uploads",
                "POST /uploads/presign",
                "POST /uploads/complete",
                "GET /uploads/:file_id/download",
                "GET /auth/start",
                "GET /auth/callback",
                "POST /logout",
            ],
        },
    )
}

async fn handle_me(context: Context) -> Response<Body> {
    if let Err(error) = ensure_schema() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    match current_session_user(&context).await {
        Ok(Some(user)) => {
            if let Err(error) = upsert_registered_user(&user) {
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error);
            }
            match user_usage_bytes(&user.sub) {
                Ok(used_bytes) => {
                    json_response(StatusCode::OK, SessionPayload::signed_in(user, used_bytes))
                }
                Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error),
            }
        }
        Ok(None) => json_response(
            StatusCode::OK,
            SessionPayload::signed_out("No active Google login session".to_owned()),
        ),
        Err(error) => json_response_with_cookies(
            StatusCode::OK,
            SessionPayload::signed_out(format!("Session expired or invalid: {error}")),
            &[clear_cookie(SESSION_COOKIE)],
        ),
    }
}

async fn handle_list_uploads(context: Context) -> Response<Body> {
    if let Err(error) = ensure_schema() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    let user = match require_authenticated_user(&context).await {
        Ok(user) => user,
        Err(error) => return json_error(StatusCode::UNAUTHORIZED, &error),
    };
    match (list_user_uploads(&user.sub), user_usage_bytes(&user.sub)) {
        (Ok(uploads), Ok(used_bytes)) => json_response(
            StatusCode::OK,
            UploadListPayload {
                limit_bytes: USER_LIMIT_BYTES,
                used_bytes,
                remaining_bytes: USER_LIMIT_BYTES.saturating_sub(used_bytes),
                uploads,
            },
        ),
        (Err(error), _) | (_, Err(error)) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

async fn handle_create_upload(context: Context) -> Response<Body> {
    if let Err(error) = ensure_schema() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    let user = match require_authenticated_user(&context).await {
        Ok(user) => user,
        Err(error) => return json_error(StatusCode::UNAUTHORIZED, &error),
    };
    let request = match read_json_body::<CreateUploadRequest>(context).await {
        Ok(request) => request,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, &error),
    };
    let filename = request.filename.trim();
    let content_type = request.content_type.trim();
    if filename.is_empty() || filename.len() > 255 {
        return json_error(
            StatusCode::BAD_REQUEST,
            "filename must be between 1 and 255 characters",
        );
    }
    if content_type.is_empty() || content_type.len() > 160 {
        return json_error(
            StatusCode::BAD_REQUEST,
            "content_type must be between 1 and 160 characters",
        );
    }
    if request.size_bytes == 0 {
        return json_error(StatusCode::BAD_REQUEST, "empty files are not accepted");
    }
    let used_bytes = match user_usage_bytes(&user.sub) {
        Ok(value) => value,
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error),
    };
    let next_used = used_bytes.saturating_add(request.size_bytes);
    if request.size_bytes > USER_LIMIT_BYTES || next_used > USER_LIMIT_BYTES {
        return json_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            LimitErrorPayload {
                error: "per-user upload limit exceeded",
                limit_bytes: USER_LIMIT_BYTES,
                used_bytes,
                remaining_bytes: USER_LIMIT_BYTES.saturating_sub(used_bytes),
                requested_bytes: request.size_bytes,
            },
        );
    }

    let presigned = match object_store::presign_upload(
        filename,
        content_type,
        request.size_bytes,
        None,
        Some(PRESIGN_EXPIRES_IN_MS),
    ) {
        Ok(value) => value,
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error),
    };
    if let Err(error) = insert_upload_record(
        &user.sub,
        &presigned.file_id,
        filename,
        content_type,
        request.size_bytes,
    ) {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    json_response(
        StatusCode::OK,
        PresignUploadPayload {
            file_id: presigned.file_id.clone(),
            upload_url: presigned.url,
            method: presigned.method,
            headers: headers_to_map(presigned.headers),
            expires_at_ms: presigned.expires_at_ms,
            limit_bytes: USER_LIMIT_BYTES,
            used_bytes_after: next_used,
            remaining_bytes_after: USER_LIMIT_BYTES.saturating_sub(next_used),
        },
    )
}

async fn handle_complete_upload(context: Context) -> Response<Body> {
    if let Err(error) = ensure_schema() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    let user = match require_authenticated_user(&context).await {
        Ok(user) => user,
        Err(error) => return json_error(StatusCode::UNAUTHORIZED, &error),
    };
    let request = match read_json_body::<CompleteUploadRequest>(context).await {
        Ok(request) => request,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, &error),
    };
    match mark_upload_completed(&user.sub, &request.file_id) {
        Ok(upload) => json_response(StatusCode::OK, upload),
        Err(error) => json_error(StatusCode::BAD_REQUEST, &error),
    }
}

async fn handle_download_upload(context: Context) -> Response<Body> {
    if let Err(error) = ensure_schema() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    let user = match require_authenticated_user(&context).await {
        Ok(user) => user,
        Err(error) => return json_error(StatusCode::UNAUTHORIZED, &error),
    };
    let Some(file_id) = context.param("file_id").map(str::to_owned) else {
        return json_error(StatusCode::BAD_REQUEST, "file_id is required");
    };
    let upload = match get_user_upload(&user.sub, &file_id) {
        Ok(upload) => upload,
        Err(error) => return json_error(StatusCode::NOT_FOUND, &error),
    };
    if upload.status != "uploaded" {
        return json_error(
            StatusCode::BAD_REQUEST,
            "upload is not marked completed yet",
        );
    }
    match object_store::presign_download(&file_id, Some(PRESIGN_EXPIRES_IN_MS)) {
        Ok(url) => json_response(StatusCode::OK, DownloadPayload::from_presigned(url)),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

async fn handle_auth_start(context: Context) -> Response<Body> {
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

async fn handle_auth_callback(context: Context) -> Response<Body> {
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

fn handle_logout() -> Response<Body> {
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

async fn current_session_user(context: &Context) -> std::result::Result<Option<UserInfo>, String> {
    let config = read_config()?;
    let Some(access_token) = cookie_value(context.request().headers(), SESSION_COOKIE) else {
        return Ok(None);
    };
    fetch_userinfo(&config, &access_token).await.map(Some)
}

async fn require_authenticated_user(context: &Context) -> std::result::Result<UserInfo, String> {
    current_session_user(context)
        .await?
        .ok_or_else(|| "login required".to_owned())
}

async fn read_json_body<T: for<'de> Deserialize<'de>>(
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

fn ensure_schema() -> std::result::Result<(), String> {
    let _ = sqlite::migrations::apply(&[
        sqlite::migrations::Migration {
            id: "001_create_registered_users",
            sql: "create table if not exists registered_users (
                sub text primary key,
                display_name text not null,
                avatar_url text not null,
                first_seen_at_ms integer not null,
                last_login_at_ms integer not null
            );",
        },
        sqlite::migrations::Migration {
            id: "002_create_uploads",
            sql: "create table if not exists uploads (
                file_id text primary key,
                user_sub text not null,
                filename text not null,
                content_type text not null,
                size_bytes integer not null,
                status text not null,
                created_at_ms integer not null,
                completed_at_ms integer
            );",
        },
        sqlite::migrations::Migration {
            id: "003_uploads_user_index",
            sql: "create index if not exists uploads_user_sub_idx on uploads (user_sub, created_at_ms desc);",
        },
    ])?;
    Ok(())
}

fn upsert_registered_user(user: &UserInfo) -> std::result::Result<(), String> {
    let now = now_ms();
    let nickname = display_nickname(user);
    let avatar_url = user.avatar_url.clone().unwrap_or_default();
    sqlite::execute(
        "insert into registered_users (sub, display_name, avatar_url, first_seen_at_ms, last_login_at_ms)
         values (?, ?, ?, ?, ?)
         on conflict(sub) do update set
           display_name = excluded.display_name,
           avatar_url = excluded.avatar_url,
           last_login_at_ms = excluded.last_login_at_ms",
        &[
            user.sub.as_str(),
            nickname.as_str(),
            avatar_url.as_str(),
            &now.to_string(),
            &now.to_string(),
        ],
    )?;
    Ok(())
}

fn insert_upload_record(
    user_sub: &str,
    file_id: &str,
    filename: &str,
    content_type: &str,
    size_bytes: u64,
) -> std::result::Result<(), String> {
    let now = now_ms();
    sqlite::execute(
        "insert into uploads (file_id, user_sub, filename, content_type, size_bytes, status, created_at_ms, completed_at_ms)
         values (?, ?, ?, ?, ?, 'pending', ?, null)",
        &[
            file_id,
            user_sub,
            filename,
            content_type,
            &size_bytes.to_string(),
            &now.to_string(),
        ],
    )?;
    Ok(())
}

fn mark_upload_completed(
    user_sub: &str,
    file_id: &str,
) -> std::result::Result<UploadRecord, String> {
    let existing = get_user_upload(user_sub, file_id)?;
    let now = now_ms();
    let now_string = now.to_string();
    sqlite::execute(
        "update uploads set status = 'uploaded', completed_at_ms = ? where user_sub = ? and file_id = ?",
        &[now_string.as_str(), user_sub, file_id],
    )?;
    Ok(UploadRecord {
        status: "uploaded".to_owned(),
        completed_at_ms: Some(now),
        ..existing
    })
}

fn user_usage_bytes(user_sub: &str) -> std::result::Result<u64, String> {
    let result = sqlite::query_typed(
        "select coalesce(sum(size_bytes), 0) from uploads where user_sub = ? and status in ('pending', 'uploaded')",
        &[user_sub],
    )?;
    let row = result
        .rows
        .first()
        .ok_or_else(|| "usage row missing".to_owned())?;
    parse_u64(row.values.first(), "used_bytes")
}

fn list_user_uploads(user_sub: &str) -> std::result::Result<Vec<UploadRecord>, String> {
    let result = sqlite::query_typed(
        "select file_id, filename, content_type, size_bytes, status, created_at_ms, completed_at_ms
         from uploads
         where user_sub = ?
         order by created_at_ms desc, file_id asc",
        &[user_sub],
    )?;
    result.rows.iter().map(upload_from_row).collect()
}

fn get_user_upload(user_sub: &str, file_id: &str) -> std::result::Result<UploadRecord, String> {
    let result = sqlite::query_typed(
        "select file_id, filename, content_type, size_bytes, status, created_at_ms, completed_at_ms
         from uploads
         where user_sub = ? and file_id = ?",
        &[user_sub, file_id],
    )?;
    result
        .rows
        .first()
        .map(upload_from_row)
        .transpose()?
        .ok_or_else(|| "upload not found".to_owned())
}

fn upload_from_row(row: &ignis_sdk::sqlite::TypedRow) -> std::result::Result<UploadRecord, String> {
    Ok(UploadRecord {
        file_id: parse_text(row.values.first(), "file_id")?,
        filename: parse_text(row.values.get(1), "filename")?,
        content_type: parse_text(row.values.get(2), "content_type")?,
        size_bytes: parse_u64(row.values.get(3), "size_bytes")?,
        status: parse_text(row.values.get(4), "status")?,
        created_at_ms: parse_i64(row.values.get(5), "created_at_ms")?,
        completed_at_ms: parse_optional_i64(row.values.get(6), "completed_at_ms")?,
    })
}

fn parse_text(value: Option<&SqliteValue>, field: &str) -> std::result::Result<String, String> {
    match value {
        Some(SqliteValue::Text(value)) => Ok(value.clone()),
        Some(other) => Err(format!("unexpected sqlite type for {field}: {other:?}")),
        None => Err(format!("missing sqlite value for {field}")),
    }
}

fn parse_i64(value: Option<&SqliteValue>, field: &str) -> std::result::Result<i64, String> {
    match value {
        Some(SqliteValue::Integer(value)) => Ok(*value),
        Some(other) => Err(format!("unexpected sqlite type for {field}: {other:?}")),
        None => Err(format!("missing sqlite value for {field}")),
    }
}

fn parse_u64(value: Option<&SqliteValue>, field: &str) -> std::result::Result<u64, String> {
    let value = parse_i64(value, field)?;
    u64::try_from(value).map_err(|_| format!("{field} cannot be negative"))
}

fn parse_optional_i64(
    value: Option<&SqliteValue>,
    field: &str,
) -> std::result::Result<Option<i64>, String> {
    match value {
        Some(SqliteValue::Integer(value)) => Ok(Some(*value)),
        Some(SqliteValue::Null) => Ok(None),
        Some(other) => Err(format!("unexpected sqlite type for {field}: {other:?}")),
        None => Err(format!("missing sqlite value for {field}")),
    }
}

fn headers_to_map(headers: Vec<object_store::Header>) -> BTreeMap<String, String> {
    headers
        .into_iter()
        .map(|header| (header.name, header.value))
        .collect()
}

fn http_client() -> Client {
    let mut client = Client::new();
    client.set_connect_timeout(Duration::from_secs(5));
    client.set_first_byte_timeout(Duration::from_secs(10));
    client.set_between_bytes_timeout(Duration::from_secs(10));
    client
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis() as i64
}

struct ExampleConfig {
    igniscloud_id_base_url: String,
    client_id: String,
    client_secret: String,
}

#[derive(Serialize)]
struct ApiRootPayload {
    name: &'static str,
    limit_bytes: u64,
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
    limit_bytes: u64,
    used_bytes: u64,
    remaining_bytes: u64,
    message: String,
}

impl SessionPayload {
    fn signed_out(message: String) -> Self {
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

    fn signed_in(user: UserInfo, used_bytes: u64) -> Self {
        Self {
            authenticated: true,
            nickname: Some(display_nickname(&user)),
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
struct CreateUploadRequest {
    filename: String,
    content_type: String,
    size_bytes: u64,
}

#[derive(Deserialize)]
struct CompleteUploadRequest {
    file_id: String,
}

#[derive(Serialize)]
struct UploadListPayload {
    limit_bytes: u64,
    used_bytes: u64,
    remaining_bytes: u64,
    uploads: Vec<UploadRecord>,
}

#[derive(Serialize)]
struct PresignUploadPayload {
    file_id: String,
    upload_url: String,
    method: String,
    headers: BTreeMap<String, String>,
    expires_at_ms: Option<u64>,
    limit_bytes: u64,
    used_bytes_after: u64,
    remaining_bytes_after: u64,
}

#[derive(Serialize)]
struct DownloadPayload {
    file_id: String,
    download_url: String,
    method: String,
    headers: BTreeMap<String, String>,
    expires_at_ms: Option<u64>,
}

impl DownloadPayload {
    fn from_presigned(url: PresignedUrl) -> Self {
        Self {
            file_id: url.file_id,
            download_url: url.url,
            method: url.method,
            headers: headers_to_map(url.headers),
            expires_at_ms: url.expires_at_ms,
        }
    }
}

#[derive(Serialize)]
struct UploadRecord {
    file_id: String,
    filename: String,
    content_type: String,
    size_bytes: u64,
    status: String,
    created_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    completed_at_ms: Option<i64>,
}

#[derive(Serialize)]
struct LimitErrorPayload {
    error: &'static str,
    limit_bytes: u64,
    used_bytes: u64,
    remaining_bytes: u64,
    requested_bytes: u64,
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

#[derive(Clone, Deserialize)]
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

fn requested_next_path(context: &Context) -> String {
    parse_query_map(context.request().uri().query())
        .get("next")
        .filter(|value| value.starts_with('/'))
        .cloned()
        .unwrap_or_else(|| "/".to_owned())
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

fn redirect_with_cookies(status: StatusCode, location: &str, cookies: &[String]) -> Response<Body> {
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

fn redirect_to_frontend_error(path: &str, message: &str) -> Response<Body> {
    let location = format!("{path}?error={}", encode(message));
    redirect_with_cookies(
        StatusCode::SEE_OTHER,
        &location,
        &[
            clear_cookie(STATE_COOKIE),
            clear_cookie(VERIFIER_COOKIE),
            clear_cookie(NEXT_COOKIE),
        ],
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
