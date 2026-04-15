use ignis_sdk::http::Context;
use ignis_sdk::object_store;
use wstd::http::{Body, Response, StatusCode};

use crate::auth::{current_session_user, require_authenticated_user};
use crate::constants::{PRESIGN_EXPIRES_IN_MS, SESSION_COOKIE, USER_LIMIT_BYTES};
use crate::db::{
    ensure_schema, get_user_upload, insert_upload_record, list_user_uploads, mark_upload_completed,
    upsert_registered_user, user_usage_bytes,
};
use crate::models::{
    ApiRootPayload, CompleteUploadRequest, CreateUploadRequest, DownloadPayload, LimitErrorPayload,
    PresignUploadPayload, SessionPayload, UploadListPayload, headers_to_map,
};
use crate::response::{clear_cookie, json_error, json_response, json_response_with_cookies};
use crate::util::read_json_body;

pub(crate) fn handle_root() -> Response<Body> {
    json_response(
        StatusCode::OK,
        ApiRootPayload {
            name: "cos-and-jobs-example-api",
            limit_bytes: USER_LIMIT_BYTES,
            endpoints: vec![
                "GET /me",
                "GET /uploads",
                "POST /uploads/presign",
                "POST /uploads/complete",
                "GET /uploads/:file_id/download",
                "POST /jobs/cleanup-pending-uploads",
                "GET /auth/start",
                "GET /auth/callback",
                "POST /logout",
            ],
        },
    )
}

pub(crate) async fn handle_me(context: Context) -> Response<Body> {
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

pub(crate) async fn handle_list_uploads(context: Context) -> Response<Body> {
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

pub(crate) async fn handle_create_upload(context: Context) -> Response<Body> {
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

pub(crate) async fn handle_complete_upload(context: Context) -> Response<Body> {
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

pub(crate) async fn handle_download_upload(context: Context) -> Response<Body> {
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
