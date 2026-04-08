use ignis_sdk::http::{Context, Router, text_response};
use wstd::http::{Body, Request, Response, Result, StatusCode};

#[wstd::http_server]
async fn main(req: Request<Body>) -> Result<Response<Body>> {
    let router = build_router();
    Ok(router.handle(req).await)
}

fn build_router() -> Router {
    let mut router = Router::new();

    router
        .get("/", |_context: Context| async move {
            text_response(StatusCode::OK, "hello world")
        })
        .expect("register GET /");

    router
        .get("/hello", |_context: Context| async move {
            text_response(StatusCode::OK, "hello world")
        })
        .expect("register GET /hello");

    router
}
