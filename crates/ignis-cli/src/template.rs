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

pub fn frontend_src_index_html(title: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{title}</title>
    <style>
      :root {{
        color-scheme: light;
        font-family: "Helvetica Neue", Helvetica, Arial, sans-serif;
        background: #f4efe7;
        color: #1f1a14;
      }}
      body {{
        margin: 0;
        min-height: 100vh;
        display: grid;
        place-items: center;
        background:
          radial-gradient(circle at top left, rgba(190, 24, 93, 0.14), transparent 28%),
          radial-gradient(circle at bottom right, rgba(14, 116, 144, 0.18), transparent 24%),
          #f4efe7;
      }}
      main {{
        width: min(720px, calc(100vw - 48px));
        padding: 40px;
        border-radius: 24px;
        background: rgba(255, 255, 255, 0.86);
        box-shadow: 0 24px 80px rgba(31, 26, 20, 0.12);
      }}
      h1 {{
        margin: 0 0 12px;
        font-size: clamp(2.2rem, 5vw, 4rem);
        line-height: 0.96;
      }}
      p {{
        margin: 0;
        font-size: 1.05rem;
        line-height: 1.6;
      }}
      code {{
        font-family: "SFMono-Regular", Consolas, monospace;
      }}
    </style>
  </head>
  <body>
    <main>
      <h1>{title}</h1>
      <p>Edit <code>src/index.html</code>, then run the frontend build command to copy it into <code>dist/</code>.</p>
    </main>
  </body>
</html>
"#
    )
}

pub fn frontend_gitignore() -> &'static str {
    "/dist\n"
}
