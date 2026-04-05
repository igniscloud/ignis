pub fn cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
publish = false

[workspace]

[lib]
crate-type = ["cdylib"]

[dependencies]
http-body-util = "0.1.3"
wstd = "0.6"
"#
    )
}

pub fn lib_rs() -> &'static str {
    r#"use wstd::http::{body::Bytes, Body, Request, Response, Result, StatusCode};
use wstd::time::{Duration, Instant};

#[wstd::http_server]
async fn main(req: Request<Body>) -> Result<Response<Body>> {
    match req.uri().path_and_query().unwrap().as_str() {
        "/wait" => wait(req).await,
        "/echo" => echo(req).await,
        "/echo-headers" => echo_headers(req).await,
        "/echo-trailers" => echo_trailers(req).await,
        "/" => home(req).await,
        _ => not_found(req).await,
    }
}

async fn home(_req: Request<Body>) -> Result<Response<Body>> {
    Ok(Response::new("Hello, wasi:http/proxy world!\n".into()))
}

async fn wait(_req: Request<Body>) -> Result<Response<Body>> {
    let now = Instant::now();
    wstd::task::sleep(Duration::from_secs(1)).await;
    let elapsed = Instant::now().duration_since(now).as_millis();
    Ok(Response::new(format!("slept for {elapsed} millis\n").into()))
}

async fn echo(req: Request<Body>) -> Result<Response<Body>> {
    Ok(Response::new(req.into_body()))
}

async fn echo_headers(req: Request<Body>) -> Result<Response<Body>> {
    let mut res = Response::builder();
    *res.headers_mut().unwrap() = req.into_parts().0.headers;
    Ok(res.body(().into()).expect("builder success"))
}

async fn echo_trailers(req: Request<Body>) -> Result<Response<Body>> {
    use http_body_util::{BodyExt, Full};

    let collected = req.into_body().into_boxed_body().collect().await?;
    let (trailers, report) = if let Some(trailers) = collected.trailers() {
        (
            Some(Ok(trailers.clone())),
            format!("received trailers: {trailers:?}"),
        )
    } else {
        (None, "request had no trailers".to_owned())
    };

    Ok(Response::new(Body::from_http_body(
        Full::new(Bytes::from(report)).with_trailers(async { trailers }),
    )))
}

async fn not_found(_req: Request<Body>) -> Result<Response<Body>> {
    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(().into())
        .expect("builder succeeds"))
}
"#
}

pub fn world_wit(name: &str) -> String {
    format!("package app:{name};\n\nworld {name} {{\n    include wasi:http/proxy@0.2.2;\n}}\n")
}

pub fn gitignore() -> &'static str {
    "/target\n"
}
