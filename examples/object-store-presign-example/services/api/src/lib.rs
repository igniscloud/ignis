use ignis_sdk::http::{Context, Router, middleware, text_response};
use ignis_sdk::object_store::{self, PresignedUrl};
use serde_json::json;
use wstd::http::{Body, Request, Response, Result, StatusCode};

const DEFAULT_EXPIRES_IN_MS: u64 = 15 * 60 * 1000;

#[wstd::http_server]
async fn main(req: Request<Body>) -> Result<Response<Body>> {
    let router = build_router();
    Ok(router.handle(req).await)
}

fn build_router() -> Router {
    let mut router = Router::new();
    router.use_middleware(middleware::request_id());
    router.use_middleware(middleware::logger());

    router
        .get("/", |_context: Context| async move {
            text_response(
                StatusCode::OK,
                "GET /presign-upload?filename=demo.txt&content_type=text/plain&size=12\nGET /presign-download/<file_id>\n",
            )
        })
        .expect("register GET /");

    router
        .get("/presign-upload", |context: Context| async move {
            let query = context.request().uri().query().unwrap_or_default();
            let filename = query_param(query, "filename").unwrap_or("demo.txt");
            let content_type = query_param(query, "content_type").unwrap_or("text/plain");
            let size_bytes = query_param(query, "size")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0);
            match object_store::presign_upload(
                filename,
                content_type,
                size_bytes,
                None,
                Some(DEFAULT_EXPIRES_IN_MS),
            ) {
                Ok(url) => json_response(StatusCode::OK, presigned_url_json("upload_url", url)),
                Err(error) => error_response(error),
            }
        })
        .expect("register GET /presign-upload");

    router
        .get(
            "/presign-download/:file_id",
            |context: Context| async move {
                let Some(file_id) = context.param("file_id") else {
                    return text_response(StatusCode::BAD_REQUEST, "missing file_id\n");
                };
                match object_store::presign_download(file_id, Some(DEFAULT_EXPIRES_IN_MS)) {
                    Ok(url) => {
                        json_response(StatusCode::OK, presigned_url_json("download_url", url))
                    }
                    Err(error) => error_response(error),
                }
            },
        )
        .expect("register GET /presign-download/:file_id");

    router
}

fn query_param<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query.split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == name).then_some(value)
    })
}

fn presigned_url_json(url_key: &str, url: PresignedUrl) -> serde_json::Value {
    let headers = url
        .headers
        .into_iter()
        .map(|header| (header.name, json!(header.value)))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "file_id": url.file_id,
        url_key: url.url,
        "method": url.method,
        "headers": headers,
        "expires_at_ms": url.expires_at_ms,
    })
}

fn json_response(status: StatusCode, value: serde_json::Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(value.to_string().into())
        .expect("json response")
}

fn error_response(error: String) -> Response<Body> {
    json_response(StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": error }))
}
