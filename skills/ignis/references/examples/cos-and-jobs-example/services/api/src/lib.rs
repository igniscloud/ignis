mod auth;
mod constants;
mod db;
mod jobs;
mod models;
mod response;
mod uploads;
mod util;

use ignis_sdk::http::{Context, Router};
use wstd::http::{Body, Method, Request, Response, Result};

#[wstd::http_server]
async fn main(req: Request<Body>) -> Result<Response<Body>> {
    let router = build_router();
    Ok(router.handle(req).await)
}

fn build_router() -> Router {
    let mut router = Router::new();

    router
        .get(
            "/",
            |_context: Context| async move { uploads::handle_root() },
        )
        .expect("register GET /");
    router
        .get("/me", |context: Context| async move {
            uploads::handle_me(context).await
        })
        .expect("register GET /me");
    router
        .get("/uploads", |context: Context| async move {
            uploads::handle_list_uploads(context).await
        })
        .expect("register GET /uploads");
    router
        .route(
            Method::POST,
            "/uploads/presign",
            |context: Context| async move { uploads::handle_create_upload(context).await },
        )
        .expect("register POST /uploads/presign");
    router
        .route(
            Method::POST,
            "/uploads/complete",
            |context: Context| async move { uploads::handle_complete_upload(context).await },
        )
        .expect("register POST /uploads/complete");
    router
        .get(
            "/uploads/:file_id/download",
            |context: Context| async move { uploads::handle_download_upload(context).await },
        )
        .expect("register GET /uploads/:file_id/download");
    router
        .route(
            Method::POST,
            "/jobs/cleanup-pending-uploads",
            |context: Context| async move { jobs::handle_cleanup_pending_uploads(context).await },
        )
        .expect("register POST /jobs/cleanup-pending-uploads");
    router
        .get("/auth/start", |context: Context| async move {
            auth::handle_auth_start(context).await
        })
        .expect("register GET /auth/start");
    router
        .get("/auth/callback", |context: Context| async move {
            auth::handle_auth_callback(context).await
        })
        .expect("register GET /auth/callback");
    router
        .route(Method::POST, "/logout", |_context: Context| async move {
            auth::handle_logout()
        })
        .expect("register POST /logout");

    router
}
