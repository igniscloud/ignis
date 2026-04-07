use wstd::http::{Body, Request, Response, Result};

#[wstd::http_server]
async fn main(_req: Request<Body>) -> Result<Response<Body>> {
    Ok(Response::new("hello world\n".into()))
}
