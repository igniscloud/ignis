use ignis_sdk::http::{Context, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wstd::http::{Body, Client, Method, Request, Response, Result, StatusCode};
use wstd::time::Duration;

const AGENT_BASE_URL: &str = "http://agent-service.svc";

#[derive(Debug, Deserialize)]
struct CreateTaskRequest {
    message: String,
}

#[derive(Debug, Serialize)]
struct CreateTaskResponse {
    task_id: String,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct JsonError<'a> {
    error: &'a str,
}

#[wstd::http_server]
async fn main(req: Request<Body>) -> Result<Response<Body>> {
    let router = build_router();
    Ok(router.handle(req).await)
}

fn build_router() -> Router {
    let mut router = Router::new();

    router
        .get("/healthz", |_context: Context| async move {
            json_response(StatusCode::OK, json!({ "ok": true }))
        })
        .expect("register GET /healthz");

    router
        .post("/tasks", handle_create_task)
        .expect("register POST /tasks");

    router
        .get("/tasks/:task_id", handle_task_status)
        .expect("register GET /tasks/:task_id");

    router
}

async fn handle_create_task(context: Context) -> Response<Body> {
    let input = match read_json_body::<CreateTaskRequest>(context).await {
        Ok(input) => input,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, &error),
    };

    let message = input.message.trim();
    if message.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "message is required");
    }

    let agent_request = json!({
        "prompt": format!(
            "Respond to the user message below. Return only the final JSON object through submit_task. User message: {message}"
        ),
        "task_result_json_schema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["message"],
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The agent response to show in the frontend."
                }
            }
        }
    });

    let response = match agent_json_request(Method::POST, "/v1/tasks", Some(agent_request)).await {
        Ok(response) => response,
        Err(error) => return json_error(StatusCode::BAD_GATEWAY, &error),
    };

    if !response.status.is_success() {
        return json_response(response.status, response.body);
    }

    let Some(task_id) = response.body.get("task_id").and_then(Value::as_str) else {
        return json_error(
            StatusCode::BAD_GATEWAY,
            "agent-service response did not include task_id",
        );
    };

    json_response(
        StatusCode::ACCEPTED,
        CreateTaskResponse {
            task_id: task_id.to_owned(),
            status: "queued",
        },
    )
}

async fn handle_task_status(context: Context) -> Response<Body> {
    let Some(task_id) = context.param("task_id") else {
        return json_error(StatusCode::BAD_REQUEST, "task_id is required");
    };

    let path = format!("/v1/tasks/{task_id}");
    match agent_json_request(Method::GET, &path, None).await {
        Ok(response) => json_response(response.status, response.body),
        Err(error) => json_error(StatusCode::BAD_GATEWAY, &error),
    }
}

#[derive(Debug)]
struct AgentResponse {
    status: StatusCode,
    body: Value,
}

async fn agent_json_request(
    method: Method,
    path: &str,
    body: Option<Value>,
) -> std::result::Result<AgentResponse, String> {
    let mut builder = Request::builder()
        .method(method)
        .uri(format!("{AGENT_BASE_URL}{path}"));

    let request_body = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(serde_json::to_string(&body).map_err(|error| error.to_string())?)
    } else {
        Body::empty()
    };

    let request = builder
        .body(request_body)
        .map_err(|error| format!("building agent request failed: {error}"))?;

    let mut response = http_client()
        .send(request)
        .await
        .map_err(|error| format!("calling agent-service failed: {error}"))?;
    let status = response.status();
    let payload = response
        .body_mut()
        .str_contents()
        .await
        .map_err(|error| format!("reading agent-service response failed: {error}"))?
        .to_owned();

    let body = if payload.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str::<Value>(&payload).unwrap_or_else(|_| json!({ "raw": payload }))
    };

    Ok(AgentResponse { status, body })
}

async fn read_json_body<T: for<'de> Deserialize<'de>>(
    context: Context,
) -> std::result::Result<T, String> {
    let mut request = context.into_request();
    let body = request
        .body_mut()
        .str_contents()
        .await
        .map_err(|error| format!("reading request body failed: {error}"))?
        .to_owned();
    serde_json::from_str(&body).map_err(|error| format!("invalid JSON body: {error}"))
}

fn http_client() -> Client {
    let mut client = Client::new();
    client.set_connect_timeout(Duration::from_secs(5));
    client.set_first_byte_timeout(Duration::from_secs(180));
    client.set_between_bytes_timeout(Duration::from_secs(30));
    client
}

fn json_response<T: Serialize>(status: StatusCode, payload: T) -> Response<Body> {
    let body = serde_json::to_string(&payload).expect("serialize json response");
    Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=utf-8")
        .body(Body::from(body))
        .expect("json response")
}

fn json_error(status: StatusCode, message: &str) -> Response<Body> {
    json_response(status, JsonError { error: message })
}
