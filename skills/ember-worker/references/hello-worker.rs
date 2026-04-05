use http_body_util::BodyExt;
use ember_sdk::http::{Context, Router, middleware, text_response};
use wstd::http::{Body, Method, Request, Response, Result, StatusCode};
use wstd::time::{Duration, Instant};

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
            text_response(StatusCode::OK, "hello from router worker\n")
        })
        .expect("register GET /");
    router
        .get("/wait", |_context: Context| async move {
            let now = Instant::now();
            wstd::task::sleep(Duration::from_secs(1)).await;
            let elapsed = Instant::now().duration_since(now).as_millis();
            text_response(StatusCode::OK, format!("slept for {elapsed} millis\n"))
        })
        .expect("register GET /wait");
    router
        .route(Method::POST, "/echo", |context: Context| async move {
            let request = context.into_request();
            Response::new(request.into_body())
        })
        .expect("register POST /echo");
    router
        .get("/echo-headers", |context: Context| async move {
            let mut lines = Vec::new();
            for (name, value) in context.request().headers() {
                let value = value.to_str().unwrap_or("<non-utf8>");
                lines.push(format!("{}: {}", name.as_str(), value));
            }
            if lines.is_empty() {
                text_response(StatusCode::OK, "no request headers\n")
            } else {
                text_response(StatusCode::OK, format!("{}\n", lines.join("\n")))
            }
        })
        .expect("register GET /echo-headers");
    router
        .get("/users/:id", |context: Context| async move {
            let user_id = context.param("id").unwrap_or("unknown");
            let request_id = context.request_id().unwrap_or("missing");
            text_response(
                StatusCode::OK,
                format!("user={user_id} request_id={request_id}\n"),
            )
        })
        .expect("register GET /users/:id");
    router
}

#[allow(dead_code)]
async fn _collect_body(context: Context) -> Result<String> {
    let request = context.into_request();
    let collected = request.into_body().into_boxed_body().collect().await?;
    Ok(String::from_utf8_lossy(&collected.to_bytes()).to_string())
}
