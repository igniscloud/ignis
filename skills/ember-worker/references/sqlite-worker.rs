use ember_sdk::http::{Context, Router, middleware, text_response};
use ember_sdk::sqlite::{self, SqliteValue};
use wstd::http::{Body, Request, Response, Result, StatusCode};

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
            if let Err(error) = ensure_schema() {
                return error_response(error);
            }
            match read_counter() {
                Ok(count) => text_response(StatusCode::OK, format!("counter={count}\n")),
                Err(error) => error_response(error),
            }
        })
        .expect("register GET /");
    router
        .post("/increment", |_context: Context| async move {
            if let Err(error) = ensure_schema() {
                return error_response(error);
            }
            match increment_counter() {
                Ok(count) => text_response(StatusCode::OK, format!("counter={count}\n")),
                Err(error) => error_response(error),
            }
        })
        .expect("register POST /increment");
    router
}

fn ensure_schema() -> std::result::Result<(), String> {
    let _ = sqlite::migrations::apply(&[
        sqlite::migrations::Migration {
            id: "001_create_counters",
            sql: "create table if not exists counters (name text primary key, value integer not null);",
        },
        sqlite::migrations::Migration {
            id: "002_seed_counter",
            sql: "insert into counters (name, value) values ('hits', 0) on conflict(name) do nothing;",
        },
    ])?;
    Ok(())
}

fn read_counter() -> std::result::Result<i64, String> {
    let result = sqlite::query_typed("select value from counters where name = ?", &["hits"])?;
    let row = result
        .rows
        .first()
        .ok_or_else(|| "counter row missing".to_owned())?;
    let value = row
        .values
        .first()
        .ok_or_else(|| "counter value missing".to_owned())?;
    match value {
        SqliteValue::Integer(value) => Ok(*value),
        other => Err(format!("unexpected counter value type: {other:?}")),
    }
}

fn increment_counter() -> std::result::Result<i64, String> {
    sqlite::execute(
        "update counters set value = value + 1 where name = ?",
        &["hits"],
    )?;
    read_counter()
}

fn error_response(error: String) -> Response<Body> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(format!("sqlite error: {error}\n").into())
        .expect("error response")
}
