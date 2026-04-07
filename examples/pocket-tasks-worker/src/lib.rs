use std::time::{SystemTime, UNIX_EPOCH};

use http::header::{CONTENT_TYPE, HeaderValue};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use ignis_sdk::http::{Context as HttpContext, Router, middleware};
use ignis_sdk::sqlite;
use wstd::http::{Body, Request, Response, Result, StatusCode};

#[derive(Debug, Serialize)]
struct Task {
    id: String,
    title: String,
    done: bool,
    created_at_ms: i64,
}

#[derive(Debug, Serialize)]
struct TaskStats {
    total: usize,
    open: usize,
    done: usize,
}

#[derive(Debug, Serialize)]
struct TaskList {
    items: Vec<Task>,
    stats: TaskStats,
}

#[derive(Debug, Deserialize)]
struct CreateTaskInput {
    title: String,
}

#[wstd::http_server]
async fn main(req: Request<Body>) -> Result<Response<Body>> {
    Ok(handle(req).await)
}

async fn handle(req: Request<Body>) -> Response<Body> {
    if let Err(error) = ensure_schema() {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }

    let router = build_router();
    router.handle(req).await
}

fn build_router() -> Router {
    let mut router = Router::new();
    router.use_middleware(middleware::request_id());
    router.use_middleware(middleware::logger());
    router.use_middleware(middleware::cors());
    router.get("/", index).expect("register GET /");
    router
        .get("/api/tasks", list_tasks_handler)
        .expect("register GET /api/tasks");
    router
        .post("/api/tasks", create_task)
        .expect("register POST /api/tasks");
    router
        .post("/api/tasks/:id/toggle", toggle_task_handler)
        .expect("register POST /api/tasks/:id/toggle");
    router
        .delete("/api/tasks/:id", delete_task_handler)
        .expect("register DELETE /api/tasks/:id");
    router
}

async fn index(_context: HttpContext) -> Response<Body> {
    json_response(
        StatusCode::OK,
        json!({
            "data": {
                "name": "Pocket Tasks API",
                "routes": [
                    "GET /api/tasks",
                    "POST /api/tasks",
                    "POST /api/tasks/{id}/toggle",
                    "DELETE /api/tasks/{id}"
                ]
            }
        }),
    )
}

async fn list_tasks_handler(_context: HttpContext) -> Response<Body> {
    match list_tasks() {
        Ok(payload) => json_response(StatusCode::OK, json!({ "data": payload })),
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

async fn create_task(context: HttpContext) -> Response<Body> {
    let input = match read_json::<CreateTaskInput>(context.into_request()).await {
        Ok(input) => input,
        Err(error) => return error_response(StatusCode::BAD_REQUEST, &error),
    };
    let title = input.title.trim();
    if title.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "title is required");
    }
    if title.chars().count() > 80 {
        return error_response(StatusCode::BAD_REQUEST, "title is too long");
    }

    let created_at_ms = now_ms();
    let id = format!("task-{}", now_nanos());
    match insert_task(&id, title, created_at_ms) {
        Ok(task) => json_response(StatusCode::CREATED, json!({ "data": task })),
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

async fn toggle_task_handler(context: HttpContext) -> Response<Body> {
    let Some(id) = context.param("id") else {
        return error_response(StatusCode::BAD_REQUEST, "missing task id");
    };
    match toggle_task(id) {
        Ok(task) => json_response(StatusCode::OK, json!({ "data": task })),
        Err(error) if error == "task not found" => error_response(StatusCode::NOT_FOUND, &error),
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

async fn delete_task_handler(context: HttpContext) -> Response<Body> {
    let Some(id) = context.param("id") else {
        return error_response(StatusCode::BAD_REQUEST, "missing task id");
    };
    match delete_task(id) {
        Ok(()) => empty_response(StatusCode::NO_CONTENT),
        Err(error) if error == "task not found" => error_response(StatusCode::NOT_FOUND, &error),
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

async fn read_json<T: for<'de> Deserialize<'de>>(
    req: Request<Body>,
) -> std::result::Result<T, String> {
    let collected = req
        .into_body()
        .into_boxed_body()
        .collect()
        .await
        .map_err(|error| format!("reading request body failed: {error}"))?;
    let bytes = collected.to_bytes();
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid json body: {error}"))
}

fn ensure_schema() -> std::result::Result<(), String> {
    sqlite::execute(
        "create table if not exists tasks (
            id text primary key,
            title text not null,
            done integer not null default 0,
            created_at_ms integer not null
        )",
        &[] as &[&str],
    )?;

    let count = query_scalar_i64("select count(*) from tasks", &[] as &[&str])?;
    if count == 0 {
        sqlite::execute(
            "insert into tasks (id, title, done, created_at_ms) values (?, ?, ?, ?)",
            &[
                "demo-setup",
                "Connect the Vue form to the worker",
                "1",
                "1735689600000",
            ],
        )?;
        sqlite::execute(
            "insert into tasks (id, title, done, created_at_ms) values (?, ?, ?, ?)",
            &[
                "demo-ship",
                "Ship one SQLite-backed action",
                "0",
                "1735689601000",
            ],
        )?;
    }
    Ok(())
}

fn list_tasks() -> std::result::Result<TaskList, String> {
    let result = sqlite::query(
        "select id, title, done, created_at_ms from tasks order by created_at_ms desc",
        &[] as &[&str],
    )?;
    let mut items = Vec::with_capacity(result.rows.len());
    for row in result.rows {
        items.push(task_from_row(&row.values)?);
    }

    let total = items.len();
    let done = items.iter().filter(|task| task.done).count();
    let open = total.saturating_sub(done);
    Ok(TaskList {
        items,
        stats: TaskStats { total, open, done },
    })
}

fn insert_task(id: &str, title: &str, created_at_ms: i64) -> std::result::Result<Task, String> {
    sqlite::execute(
        "insert into tasks (id, title, done, created_at_ms) values (?, ?, ?, ?)",
        &[id, title, "0", &created_at_ms.to_string()],
    )?;
    fetch_task(id)
}

fn toggle_task(id: &str) -> std::result::Result<Task, String> {
    let count = sqlite::execute(
        "update tasks set done = case when done = 1 then 0 else 1 end where id = ?",
        &[id],
    )?;
    if count == 0 {
        return Err("task not found".to_owned());
    }
    fetch_task(id)
}

fn delete_task(id: &str) -> std::result::Result<(), String> {
    let count = sqlite::execute("delete from tasks where id = ?", &[id])?;
    if count == 0 {
        return Err("task not found".to_owned());
    }
    Ok(())
}

fn fetch_task(id: &str) -> std::result::Result<Task, String> {
    let result = sqlite::query(
        "select id, title, done, created_at_ms from tasks where id = ? limit 1",
        &[id],
    )?;
    let row = result
        .rows
        .first()
        .ok_or_else(|| "task not found".to_owned())?;
    task_from_row(&row.values)
}

fn task_from_row(values: &[String]) -> std::result::Result<Task, String> {
    if values.len() < 4 {
        return Err("sqlite row shape mismatch".to_owned());
    }
    let done = match values[2].as_str() {
        "0" => false,
        "1" => true,
        other => return Err(format!("invalid done flag: {other}")),
    };
    let created_at_ms = values[3]
        .parse::<i64>()
        .map_err(|error| format!("invalid created_at_ms: {error}"))?;
    Ok(Task {
        id: values[0].clone(),
        title: values[1].clone(),
        done,
        created_at_ms,
    })
}

fn query_scalar_i64(sql: &str, params: &[impl AsRef<str>]) -> std::result::Result<i64, String> {
    let result = sqlite::query(sql, params)?;
    let row = result
        .rows
        .first()
        .ok_or_else(|| "query returned no rows".to_owned())?;
    let value = row
        .values
        .first()
        .ok_or_else(|| "query returned no values".to_owned())?;
    value
        .parse::<i64>()
        .map_err(|error| format!("invalid integer result: {error}"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn json_response(status: StatusCode, payload: serde_json::Value) -> Response<Body> {
    let mut response = Response::builder()
        .status(status)
        .body(payload.to_string().into())
        .expect("json response");
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

fn empty_response(status: StatusCode) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(().into())
        .expect("empty response")
}

fn error_response(status: StatusCode, message: &str) -> Response<Body> {
    json_response(status, json!({ "error": message }))
}
