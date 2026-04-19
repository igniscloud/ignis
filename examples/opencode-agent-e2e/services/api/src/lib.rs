use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use ignis_sdk::http::{Context, Router};
use ignis_sdk::sqlite::{self, SqliteValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use taskplan::{
    TaskPlan, TaskState, apply_output_bindings, ready_task_ids, validate_plan, validate_task_output,
};
use wstd::http::{Body, Client, Method, Request, Response, Result, StatusCode};
use wstd::time::Duration;

const SYSTEM_BASE_URL: &str = "http://__ignis.svc";
const COORDINATOR_AGENT: &str = "coordinator-agent";
const TOOL_CALLBACK_URL: &str = "http://api.svc/internal/taskplan/tools";

#[derive(Debug, Deserialize)]
struct CreateWorkflowRequest {
    #[serde(default)]
    question: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateWorkflowResponse {
    run_id: String,
    status: String,
    title: String,
}

#[derive(Debug, Serialize)]
struct JsonError<'a> {
    error: &'a str,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AvailableAgent {
    name: String,
    description: String,
    runtime: Option<String>,
    memory: Option<String>,
    service_url: String,
}

#[derive(Debug, Deserialize)]
struct ServiceDiscoveryResponse {
    data: Vec<ServiceMetadata>,
}

#[derive(Debug, Deserialize)]
struct ServiceMetadata {
    service: String,
    kind: String,
    #[serde(default)]
    service_url: Option<String>,
    #[serde(default)]
    runtime: Option<String>,
    #[serde(default)]
    memory: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolCallbackRequest {
    tool: String,
    task_id: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    task_plan: Option<Value>,
}

#[derive(Debug, Clone)]
struct RunRecord {
    run_id: String,
    question: String,
    status: String,
    available_agents_json: String,
    child_plan_json: Option<String>,
    final_result_json: Option<String>,
    error_json: Option<String>,
}

#[derive(Debug)]
struct InvocationRecord {
    run_id: String,
    plan_task_id: String,
    invocation_kind: String,
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
        .post("/workflows", handle_create_workflow)
        .expect("register POST /workflows");

    router
        .get("/workflows/:run_id", handle_workflow_status)
        .expect("register GET /workflows/:run_id");

    router
        .post("/internal/taskplan/tools", handle_tool_callback)
        .expect("register POST /internal/taskplan/tools");

    router
}

async fn handle_create_workflow(context: Context) -> Response<Body> {
    if let Err(error) = ensure_schema() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }

    let input = match read_json_body::<CreateWorkflowRequest>(context).await {
        Ok(input) => input,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, &error),
    };
    let question = input
        .question
        .unwrap_or_else(default_question)
        .trim()
        .to_owned();
    if question.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "question is required");
    }

    match create_workflow(&question).await {
        Ok(response) => json_response(StatusCode::ACCEPTED, response),
        Err(error) => json_error(StatusCode::BAD_GATEWAY, &error),
    }
}

async fn handle_workflow_status(context: Context) -> Response<Body> {
    if let Err(error) = ensure_schema() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    let Some(run_id) = context.param("run_id") else {
        return json_error(StatusCode::BAD_REQUEST, "run_id is required");
    };

    if let Err(error) = refresh_running_invocations(run_id).await {
        return json_error(StatusCode::BAD_GATEWAY, &error);
    }
    match workflow_status(run_id) {
        Ok(status) => json_response(StatusCode::OK, status),
        Err(error) => json_error(StatusCode::NOT_FOUND, &error),
    }
}

async fn handle_tool_callback(context: Context) -> Response<Body> {
    if let Err(error) = ensure_schema() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    let payload = match read_json_body::<ToolCallbackRequest>(context).await {
        Ok(payload) => payload,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, &error),
    };

    let result = match payload.tool.as_str() {
        "spawn_task_plan" => handle_spawn_task_plan(payload).await,
        "submit_task" => handle_submit_task(payload).await,
        other => Err(format!("unsupported tool callback `{other}`")),
    };

    match result {
        Ok(response) => json_response(StatusCode::OK, response),
        Err(error) => json_error(StatusCode::BAD_REQUEST, &error),
    }
}

async fn create_workflow(question: &str) -> std::result::Result<CreateWorkflowResponse, String> {
    let run_id = format!("flt-{}", now_ms());
    let available_agents = discover_available_agents().await;
    let available_agents_json =
        serde_json::to_string(&available_agents).map_err(|error| error.to_string())?;

    sqlite::execute(
        "insert into workflow_runs (
            run_id, question, status, available_agents_json, created_at_ms, updated_at_ms
         ) values (?, ?, 'coordinating', ?, ?, ?)",
        &[
            run_id.as_str(),
            question,
            available_agents_json.as_str(),
            &now_ms().to_string(),
            &now_ms().to_string(),
        ],
    )?;

    let prompt = coordinator_initial_prompt(question, &available_agents);
    let task_id = create_agent_task(COORDINATOR_AGENT, &prompt, final_result_schema()).await?;
    sqlite::execute(
        "update workflow_runs
         set coordinator_agent_task_id = ?, current_agent_task_id = ?, updated_at_ms = ?
         where run_id = ?",
        &[
            task_id.as_str(),
            task_id.as_str(),
            &now_ms().to_string(),
            run_id.as_str(),
        ],
    )?;
    insert_invocation(
        &task_id,
        &run_id,
        "root",
        COORDINATOR_AGENT,
        "coordinator_initial",
        "running",
    )?;

    Ok(CreateWorkflowResponse {
        run_id,
        status: "coordinating".to_owned(),
        title: "费马大定理高中生解法".to_owned(),
    })
}

async fn handle_spawn_task_plan(
    payload: ToolCallbackRequest,
) -> std::result::Result<Value, String> {
    let invocation = get_invocation(&payload.task_id)?;
    if !invocation.invocation_kind.starts_with("coordinator") {
        return Err("only the coordinator agent may spawn child plans".to_owned());
    }
    let task_plan_value = payload
        .task_plan
        .ok_or_else(|| "spawn_task_plan callback missing task_plan".to_owned())?;
    let task_plan: TaskPlan =
        serde_json::from_value(task_plan_value.clone()).map_err(|error| error.to_string())?;
    validate_plan(&task_plan).map_err(|error| error.to_string())?;

    sqlite::execute(
        "update workflow_runs
         set child_plan_json = ?, status = 'running_children', updated_at_ms = ?
         where run_id = ?",
        &[
            serde_json::to_string(&task_plan)
                .map_err(|error| error.to_string())?
                .as_str(),
            &now_ms().to_string(),
            invocation.run_id.as_str(),
        ],
    )?;
    sqlite::execute(
        "update task_invocations set status = 'waiting_child_plan' where agent_task_id = ?",
        &[payload.task_id.as_str()],
    )?;
    upsert_child_tasks(&invocation.run_id, &task_plan)?;
    let dispatched = dispatch_ready_tasks(&invocation.run_id).await?;

    Ok(json!({
        "accepted": true,
        "run_id": invocation.run_id,
        "child_plan_id": task_plan.id,
        "dispatched_tasks": dispatched
    }))
}

async fn handle_submit_task(payload: ToolCallbackRequest) -> std::result::Result<Value, String> {
    let invocation = get_invocation(&payload.task_id)?;
    let result = payload
        .result
        .ok_or_else(|| "submit_task callback missing result".to_owned())?;
    let status = payload.status.unwrap_or_else(|| "succeeded".to_owned());
    if status != "succeeded" {
        mark_run_failed(
            &invocation.run_id,
            json!({ "status": status, "result": result }),
        )?;
        return Ok(json!({ "ok": true, "run_id": invocation.run_id }));
    }

    if invocation.invocation_kind.starts_with("coordinator") {
        sqlite::execute(
            "update task_invocations set status = 'succeeded' where agent_task_id = ?",
            &[payload.task_id.as_str()],
        )?;
        sqlite::execute(
            "update workflow_runs
             set status = 'succeeded', final_result_json = ?, updated_at_ms = ?
             where run_id = ?",
            &[
                serde_json::to_string(&result)
                    .map_err(|error| error.to_string())?
                    .as_str(),
                &now_ms().to_string(),
                invocation.run_id.as_str(),
            ],
        )?;
        return Ok(json!({ "ok": true, "run_id": invocation.run_id }));
    }

    let run = get_run(&invocation.run_id)?;
    let plan = run_child_plan(&run)?;
    let task = plan
        .tasks
        .iter()
        .find(|task| task.id == invocation.plan_task_id)
        .ok_or_else(|| format!("plan task `{}` not found", invocation.plan_task_id))?;
    validate_task_output(task, &result).map_err(|error| error.to_string())?;

    sqlite::execute(
        "update child_tasks
         set state = 'succeeded', output_json = ?, updated_at_ms = ?
         where run_id = ? and task_id = ?",
        &[
            serde_json::to_string(&result)
                .map_err(|error| error.to_string())?
                .as_str(),
            &now_ms().to_string(),
            invocation.run_id.as_str(),
            invocation.plan_task_id.as_str(),
        ],
    )?;
    sqlite::execute(
        "update task_invocations set status = 'succeeded' where agent_task_id = ?",
        &[payload.task_id.as_str()],
    )?;

    let dispatched = dispatch_ready_tasks(&invocation.run_id).await?;
    if all_child_tasks_succeeded(&invocation.run_id)?
        && !has_coordinator_continuation(&invocation.run_id)?
    {
        let coordinator_task_id = invoke_coordinator_continuation(&invocation.run_id).await?;
        return Ok(json!({
            "ok": true,
            "run_id": invocation.run_id,
            "dispatched_tasks": dispatched,
            "coordinator_task_id": coordinator_task_id
        }));
    }

    Ok(json!({
        "ok": true,
        "run_id": invocation.run_id,
        "dispatched_tasks": dispatched
    }))
}

async fn dispatch_ready_tasks(run_id: &str) -> std::result::Result<Vec<String>, String> {
    let run = get_run(run_id)?;
    let plan = run_child_plan(&run)?;
    let states = child_task_states(run_id)?;
    let ready = ready_task_ids(&plan, &states).map_err(|error| error.to_string())?;
    let outputs = child_task_outputs(run_id)?;
    let mut dispatched = Vec::new();

    for task_id in ready {
        let Some(task) = plan.tasks.iter().find(|task| task.id == task_id) else {
            continue;
        };
        let state = states.get(&task_id).copied().unwrap_or(TaskState::Queued);
        if state != TaskState::Queued {
            continue;
        }
        let input = apply_output_bindings(&plan, &task.id, &task.input, &outputs)
            .map_err(|error| error.to_string())?;
        sqlite::execute(
            "update child_tasks
             set state = 'running', input_json = ?, updated_at_ms = ?
             where run_id = ? and task_id = ?",
            &[
                serde_json::to_string(&input)
                    .map_err(|error| error.to_string())?
                    .as_str(),
                &now_ms().to_string(),
                run_id,
                task.id.as_str(),
            ],
        )?;

        let prompt = child_task_prompt(&run, task, &input);
        let agent_task_id =
            create_agent_task(&task.agent_service, &prompt, task.output_schema.clone()).await?;
        insert_invocation(
            &agent_task_id,
            run_id,
            &task.id,
            &task.agent_service,
            "child",
            "running",
        )?;
        dispatched.push(task.id.clone());
    }

    Ok(dispatched)
}

async fn invoke_coordinator_continuation(run_id: &str) -> std::result::Result<String, String> {
    let run = get_run(run_id)?;
    let plan = run_child_plan(&run)?;
    let outputs = child_task_outputs(run_id)?;
    let prompt = coordinator_continuation_prompt(&run, &plan, &outputs)?;
    let task_id = create_agent_task(COORDINATOR_AGENT, &prompt, final_result_schema()).await?;
    sqlite::execute(
        "update workflow_runs
         set status = 'synthesizing', current_agent_task_id = ?, updated_at_ms = ?
         where run_id = ?",
        &[task_id.as_str(), &now_ms().to_string(), run_id],
    )?;
    insert_invocation(
        &task_id,
        run_id,
        "root",
        COORDINATOR_AGENT,
        "coordinator_continuation",
        "running",
    )?;
    Ok(task_id)
}

async fn create_agent_task(
    agent_service: &str,
    prompt: &str,
    schema: Value,
) -> std::result::Result<String, String> {
    let request = json!({
        "prompt": prompt,
        "tool_callback_url": TOOL_CALLBACK_URL,
        "task_result_json_schema": schema
    });
    let response = json_request(
        Method::POST,
        &format!("http://{agent_service}.svc/v1/tasks"),
        Some(request),
    )
    .await?;
    if !response.status.is_success() {
        return Err(format!(
            "{agent_service} returned HTTP {}: {}",
            response.status, response.body
        ));
    }
    response
        .body
        .get("task_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{agent_service} response did not include task_id"))
}

async fn refresh_running_invocations(run_id: &str) -> std::result::Result<(), String> {
    for (agent_task_id, agent_service, status) in running_invocations(run_id)? {
        if status == "waiting_child_plan" {
            continue;
        }
        let response = json_request(
            Method::GET,
            &format!("http://{agent_service}.svc/v1/tasks/{agent_task_id}"),
            None,
        )
        .await;
        let Ok(response) = response else {
            continue;
        };
        let Some(remote_status) = response.body.get("status").and_then(Value::as_str) else {
            continue;
        };
        if remote_status == "failed" {
            mark_run_failed(
                run_id,
                json!({
                    "agent_service": agent_service,
                    "agent_task_id": agent_task_id,
                    "error": response.body.get("error").cloned().unwrap_or(Value::Null)
                }),
            )?;
        } else if remote_status == "waiting_child_plan" {
            sqlite::execute(
                "update task_invocations set status = 'waiting_child_plan' where agent_task_id = ?",
                &[agent_task_id.as_str()],
            )?;
        }
    }
    Ok(())
}

async fn discover_available_agents() -> Vec<AvailableAgent> {
    let response = json_request(Method::GET, &format!("{SYSTEM_BASE_URL}/v1/services"), None).await;
    if let Ok(response) = response {
        if response.status.is_success() {
            if let Ok(discovery) = serde_json::from_value::<ServiceDiscoveryResponse>(response.body)
            {
                let agents = discovery
                    .data
                    .into_iter()
                    .filter(|service| {
                        service.kind == "agent" && service.service != COORDINATOR_AGENT
                    })
                    .map(|service| AvailableAgent {
                        name: service.service.clone(),
                        description: service
                            .description
                            .unwrap_or_else(|| format!("Specialist agent `{}`", service.service)),
                        runtime: service.runtime,
                        memory: service.memory,
                        service_url: service
                            .service_url
                            .unwrap_or_else(|| format!("http://{}.svc", service.service)),
                    })
                    .collect::<Vec<_>>();
                if !agents.is_empty() {
                    return agents;
                }
            }
        }
    }
    fallback_available_agents()
}

fn ensure_schema() -> std::result::Result<(), String> {
    let _ = sqlite::migrations::apply(&[
        sqlite::migrations::Migration {
            id: "001_create_fermat_workflow_runs",
            sql: "
                create table if not exists workflow_runs (
                run_id text primary key,
                question text not null,
                status text not null,
                coordinator_agent_task_id text,
                current_agent_task_id text,
                available_agents_json text not null,
                child_plan_json text,
                final_result_json text,
                error_json text,
                created_at_ms integer not null,
                updated_at_ms integer not null
            );",
        },
        sqlite::migrations::Migration {
            id: "002_create_fermat_task_invocations",
            sql: "
                create table if not exists task_invocations (
                agent_task_id text primary key,
                run_id text not null,
                plan_task_id text not null,
                agent_service text not null,
                invocation_kind text not null,
                status text not null,
                created_at_ms integer not null
            );",
        },
        sqlite::migrations::Migration {
            id: "003_create_fermat_child_tasks",
            sql: "
                create table if not exists child_tasks (
                run_id text not null,
                task_id text not null,
                agent_service text not null,
                state text not null,
                input_json text not null,
                output_schema_json text not null,
                output_json text,
                error_json text,
                created_at_ms integer not null,
                updated_at_ms integer not null,
                primary key (run_id, task_id)
            );",
        },
    ])?;
    Ok(())
}

fn workflow_status(run_id: &str) -> std::result::Result<Value, String> {
    let run = get_run(run_id)?;
    let child_tasks = list_child_tasks(run_id)?;
    let invocations = list_invocations(run_id)?;
    Ok(json!({
        "run_id": run.run_id,
        "title": "费马大定理高中生解法",
        "question": run.question,
        "status": run.status,
        "available_agents": serde_json::from_str::<Value>(&run.available_agents_json).unwrap_or_else(|_| json!([])),
        "child_plan": run.child_plan_json
            .as_deref()
            .and_then(|value| serde_json::from_str::<Value>(value).ok()),
        "child_tasks": child_tasks,
        "invocations": invocations,
        "result": run.final_result_json
            .as_deref()
            .and_then(|value| serde_json::from_str::<Value>(value).ok()),
        "error": run.error_json
            .as_deref()
            .and_then(|value| serde_json::from_str::<Value>(value).ok())
    }))
}

fn upsert_child_tasks(run_id: &str, plan: &TaskPlan) -> std::result::Result<(), String> {
    for task in &plan.tasks {
        sqlite::execute(
            "insert into child_tasks (
                run_id, task_id, agent_service, state, input_json, output_schema_json,
                created_at_ms, updated_at_ms
             ) values (?, ?, ?, 'queued', ?, ?, ?, ?)
             on conflict(run_id, task_id) do nothing",
            &[
                run_id,
                task.id.as_str(),
                task.agent_service.as_str(),
                serde_json::to_string(&task.input)
                    .map_err(|error| error.to_string())?
                    .as_str(),
                serde_json::to_string(&task.output_schema)
                    .map_err(|error| error.to_string())?
                    .as_str(),
                &now_ms().to_string(),
                &now_ms().to_string(),
            ],
        )?;
    }
    Ok(())
}

fn insert_invocation(
    agent_task_id: &str,
    run_id: &str,
    plan_task_id: &str,
    agent_service: &str,
    invocation_kind: &str,
    status: &str,
) -> std::result::Result<(), String> {
    sqlite::execute(
        "insert into task_invocations (
            agent_task_id, run_id, plan_task_id, agent_service, invocation_kind, status, created_at_ms
         ) values (?, ?, ?, ?, ?, ?, ?)",
        &[
            agent_task_id,
            run_id,
            plan_task_id,
            agent_service,
            invocation_kind,
            status,
            &now_ms().to_string(),
        ],
    )?;
    Ok(())
}

fn get_run(run_id: &str) -> std::result::Result<RunRecord, String> {
    let result = sqlite::query_typed(
        "select run_id, question, status, available_agents_json, child_plan_json, final_result_json, error_json
         from workflow_runs where run_id = ?",
        &[run_id],
    )?;
    result
        .rows
        .first()
        .map(run_from_row)
        .transpose()?
        .ok_or_else(|| format!("workflow `{run_id}` not found"))
}

fn get_invocation(agent_task_id: &str) -> std::result::Result<InvocationRecord, String> {
    let result = sqlite::query_typed(
        "select run_id, plan_task_id, agent_service, invocation_kind, status
         from task_invocations where agent_task_id = ?",
        &[agent_task_id],
    )?;
    result
        .rows
        .first()
        .map(invocation_from_row)
        .transpose()?
        .ok_or_else(|| format!("agent task `{agent_task_id}` is not tracked"))
}

fn run_child_plan(run: &RunRecord) -> std::result::Result<TaskPlan, String> {
    let json = run
        .child_plan_json
        .as_deref()
        .ok_or_else(|| "workflow has no child plan yet".to_owned())?;
    serde_json::from_str(json).map_err(|error| error.to_string())
}

fn child_task_states(run_id: &str) -> std::result::Result<BTreeMap<String, TaskState>, String> {
    let result = sqlite::query_typed(
        "select task_id, state from child_tasks where run_id = ?",
        &[run_id],
    )?;
    let mut states = BTreeMap::new();
    for row in &result.rows {
        states.insert(
            parse_text(row.values.first(), "task_id")?,
            parse_task_state(&parse_text(row.values.get(1), "state")?)?,
        );
    }
    Ok(states)
}

fn child_task_outputs(run_id: &str) -> std::result::Result<BTreeMap<String, Value>, String> {
    let result = sqlite::query_typed(
        "select task_id, output_json from child_tasks
         where run_id = ? and output_json is not null",
        &[run_id],
    )?;
    let mut outputs = BTreeMap::new();
    for row in &result.rows {
        let task_id = parse_text(row.values.first(), "task_id")?;
        let output_json = parse_text(row.values.get(1), "output_json")?;
        outputs.insert(
            task_id,
            serde_json::from_str(&output_json).map_err(|error| error.to_string())?,
        );
    }
    Ok(outputs)
}

fn running_invocations(run_id: &str) -> std::result::Result<Vec<(String, String, String)>, String> {
    let result = sqlite::query_typed(
        "select agent_task_id, agent_service, status
         from task_invocations
         where run_id = ? and status in ('running', 'waiting_child_plan')",
        &[run_id],
    )?;
    result
        .rows
        .iter()
        .map(|row| {
            Ok((
                parse_text(row.values.first(), "agent_task_id")?,
                parse_text(row.values.get(1), "agent_service")?,
                parse_text(row.values.get(2), "status")?,
            ))
        })
        .collect()
}

fn list_child_tasks(run_id: &str) -> std::result::Result<Vec<Value>, String> {
    let result = sqlite::query_typed(
        "select task_id, agent_service, state, input_json, output_json, error_json
         from child_tasks where run_id = ? order by task_id asc",
        &[run_id],
    )?;
    result
        .rows
        .iter()
        .map(|row| {
            let input_json = parse_text(row.values.get(3), "input_json")?;
            let output_json = parse_optional_text(row.values.get(4), "output_json")?;
            let error_json = parse_optional_text(row.values.get(5), "error_json")?;
            Ok(json!({
                "task_id": parse_text(row.values.first(), "task_id")?,
                "agent_service": parse_text(row.values.get(1), "agent_service")?,
                "state": parse_text(row.values.get(2), "state")?,
                "input": serde_json::from_str::<Value>(&input_json).unwrap_or(Value::Null),
                "output": output_json.and_then(|value| serde_json::from_str::<Value>(&value).ok()),
                "error": error_json.and_then(|value| serde_json::from_str::<Value>(&value).ok())
            }))
        })
        .collect()
}

fn list_invocations(run_id: &str) -> std::result::Result<Vec<Value>, String> {
    let result = sqlite::query_typed(
        "select agent_task_id, plan_task_id, agent_service, invocation_kind, status
         from task_invocations where run_id = ? order by created_at_ms asc",
        &[run_id],
    )?;
    result
        .rows
        .iter()
        .map(|row| {
            Ok(json!({
                "agent_task_id": parse_text(row.values.first(), "agent_task_id")?,
                "plan_task_id": parse_text(row.values.get(1), "plan_task_id")?,
                "agent_service": parse_text(row.values.get(2), "agent_service")?,
                "kind": parse_text(row.values.get(3), "invocation_kind")?,
                "status": parse_text(row.values.get(4), "status")?
            }))
        })
        .collect()
}

fn all_child_tasks_succeeded(run_id: &str) -> std::result::Result<bool, String> {
    let result = sqlite::query_typed(
        "select count(*) from child_tasks where run_id = ? and state != 'succeeded'",
        &[run_id],
    )?;
    Ok(parse_i64(
        result.rows.first().and_then(|row| row.values.first()),
        "remaining_child_tasks",
    )? == 0)
}

fn has_coordinator_continuation(run_id: &str) -> std::result::Result<bool, String> {
    let result = sqlite::query_typed(
        "select count(*) from task_invocations
         where run_id = ? and invocation_kind = 'coordinator_continuation'",
        &[run_id],
    )?;
    Ok(parse_i64(
        result.rows.first().and_then(|row| row.values.first()),
        "coordinator_continuations",
    )? > 0)
}

fn mark_run_failed(run_id: &str, error: Value) -> std::result::Result<(), String> {
    sqlite::execute(
        "update workflow_runs
         set status = 'failed', error_json = ?, updated_at_ms = ?
         where run_id = ?",
        &[
            serde_json::to_string(&error)
                .map_err(|error| error.to_string())?
                .as_str(),
            &now_ms().to_string(),
            run_id,
        ],
    )?;
    Ok(())
}

#[derive(Debug)]
struct JsonHttpResponse {
    status: StatusCode,
    body: Value,
}

async fn json_request(
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> std::result::Result<JsonHttpResponse, String> {
    let mut builder = Request::builder().method(method).uri(uri);
    let request_body = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(serde_json::to_string(&body).map_err(|error| error.to_string())?)
    } else {
        Body::empty()
    };
    let request = builder
        .body(request_body)
        .map_err(|error| format!("building request failed: {error}"))?;

    let mut response = http_client()
        .send(request)
        .await
        .map_err(|error| format!("HTTP request to {uri} failed: {error}"))?;
    let status = response.status();
    let payload = response
        .body_mut()
        .str_contents()
        .await
        .map_err(|error| format!("reading response from {uri} failed: {error}"))?
        .to_owned();
    let body = if payload.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str::<Value>(&payload).unwrap_or_else(|_| json!({ "raw": payload }))
    };
    Ok(JsonHttpResponse { status, body })
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
    client.set_first_byte_timeout(Duration::from_secs(240));
    client.set_between_bytes_timeout(Duration::from_secs(30));
    client
}

fn coordinator_initial_prompt(question: &str, available_agents: &[AvailableAgent]) -> String {
    format!(
        r#"你是主 agent，负责规划“费马大定理高中生解法”的多 agent workflow。

用户目标：
{question}

重要边界：
- 不要伪造一个纯高中数学的完整严格证明。
- 你要组织子 agent 生成“高中生能看懂的证明导览”：说明反证主线、Frey 曲线桥接、Ribet 定理、Wiles 定理，以及这些黑箱定理如何给出矛盾。
- 你必须先调用 spawn_task_plan 创建子 TaskPlan；spawn 成功后立刻停止本轮，不要调用 submit_task。
- 子计划完成后，系统会用子结果重新调用你。第二轮只能合成最终 JSON，不要再 spawn。

可用子 agent：
{agents}

请调用 spawn_task_plan，task_plan 必须是 JSON，字段为：
- id
- root_task_id
- tasks: 每个 task 包含 id, agent_service, prompt, input, output_schema
- dependencies: 可为空；需要串联时使用 from_task, to_task, bindings

推荐子任务：
1. elementary-foundation -> elementary-agent
2. frey-ribet-bridge -> bridge-agent
3. modularity-black-box -> modularity-agent
4. teacher-draft -> teacher-agent，依赖前三个输出
5. rigor-review -> rigor-agent，依赖 teacher-draft 输出

建议直接使用这个 TaskPlan 结构，并只在 prompt 文案上小幅调整：
{{
  "id": "fermats-high-school-guide",
  "root_task_id": "rigor-review",
  "tasks": [
    {{
      "id": "elementary-foundation",
      "agent_service": "elementary-agent",
      "prompt": "Explain the high-school-accessible foundation for Fermat's Last Theorem: statement, contradiction, coprime reduction, prime exponent reduction, and the n=4 infinite descent sketch. Clearly say this does not prove the full theorem.",
      "input": {{}},
      "output_schema": {{"type":"object","additionalProperties":false,"required":["summary","key_points","boundary"],"properties":{{"summary":{{"type":"string"}},"key_points":{{"type":"array","items":{{"type":"string"}}}},"boundary":{{"type":"string"}}}}}}
    }},
    {{
      "id": "frey-ribet-bridge",
      "agent_service": "bridge-agent",
      "prompt": "Explain how a hypothetical Fermat counterexample creates the Frey curve and how Ribet's theorem turns that into a non-modularity statement. Keep Ribet's theorem as a black box.",
      "input": {{}},
      "output_schema": {{"type":"object","additionalProperties":false,"required":["summary","chain","black_box"],"properties":{{"summary":{{"type":"string"}},"chain":{{"type":"array","items":{{"type":"string"}}}},"black_box":{{"type":"string"}}}}}}
    }},
    {{
      "id": "modularity-black-box",
      "agent_service": "modularity-agent",
      "prompt": "Explain modularity and Wiles's theorem with accessible analogies, then connect Wiles's result to the contradiction with Ribet's theorem.",
      "input": {{}},
      "output_schema": {{"type":"object","additionalProperties":false,"required":["summary","analogies","black_box"],"properties":{{"summary":{{"type":"string"}},"analogies":{{"type":"array","items":{{"type":"string"}}}},"black_box":{{"type":"string"}}}}}}
    }},
    {{
      "id": "teacher-draft",
      "agent_service": "teacher-agent",
      "prompt": "Combine the elementary, Frey-Ribet, and modularity outputs into a coherent high-school-readable proof guide draft. Label black-box theorems clearly.",
      "input": {{}},
      "output_schema": {{"type":"object","additionalProperties":false,"required":["draft_title","sections","black_box_theorems"],"properties":{{"draft_title":{{"type":"string"}},"sections":{{"type":"array","items":{{"type":"object","additionalProperties":false,"required":["heading","body"],"properties":{{"heading":{{"type":"string"}},"body":{{"type":"string"}}}}}}}},"black_box_theorems":{{"type":"array","items":{{"type":"object","additionalProperties":false,"required":["name","plain_language"],"properties":{{"name":{{"type":"string"}},"plain_language":{{"type":"string"}}}}}}}}}}}}
    }},
    {{
      "id": "rigor-review",
      "agent_service": "rigor-agent",
      "prompt": "Review the teacher draft for overclaiming, missing black-box labels, incorrect contradiction logic, and unclear explanations. Return concrete rigor notes.",
      "input": {{}},
      "output_schema": {{"type":"object","additionalProperties":false,"required":["approved","rigor_notes","required_fixes"],"properties":{{"approved":{{"type":"boolean"}},"rigor_notes":{{"type":"array","items":{{"type":"string"}}}},"required_fixes":{{"type":"array","items":{{"type":"string"}}}}}}}}
    }}
  ],
  "dependencies": [
    {{"from_task":"elementary-foundation","to_task":"teacher-draft","bindings":[{{"from_pointer":"","to_pointer":"/elementary"}}]}},
    {{"from_task":"frey-ribet-bridge","to_task":"teacher-draft","bindings":[{{"from_pointer":"","to_pointer":"/frey_ribet"}}]}},
    {{"from_task":"modularity-black-box","to_task":"teacher-draft","bindings":[{{"from_pointer":"","to_pointer":"/modularity"}}]}},
    {{"from_task":"teacher-draft","to_task":"rigor-review","bindings":[{{"from_pointer":"","to_pointer":"/teacher_draft"}}]}}
  ]
}}
"#,
        question = question,
        agents = serde_json::to_string_pretty(available_agents).unwrap_or_else(|_| "[]".to_owned())
    )
}

fn child_task_prompt(run: &RunRecord, task: &taskplan::TaskSpec, input: &Value) -> String {
    format!(
        r#"你是 `{agent}`，正在执行费马大定理高中生解法 workflow 的子任务。

总目标：
{question}

你的任务：
{prompt}

输入 JSON：
{input}

输出要求：
- 只通过 submit_task 提交最终 JSON。
- 不要调用 spawn_task_plan。
- 不要假装给出了 Wiles 证明的高中初等证明；高等数学部分必须标注为黑箱。
- 输出必须匹配下面的 JSON Schema：
{schema}
"#,
        agent = task.agent_service,
        question = run.question,
        prompt = task.prompt,
        input = serde_json::to_string_pretty(input).unwrap_or_else(|_| "{}".to_owned()),
        schema =
            serde_json::to_string_pretty(&task.output_schema).unwrap_or_else(|_| "{}".to_owned())
    )
}

fn coordinator_continuation_prompt(
    run: &RunRecord,
    plan: &TaskPlan,
    outputs: &BTreeMap<String, Value>,
) -> std::result::Result<String, String> {
    Ok(format!(
        r#"你是主 agent。子 TaskPlan 已经完成，现在请合成最终的“费马大定理高中生解法”。

用户目标：
{question}

子 TaskPlan：
{plan}

子 agent 输出：
{outputs}

最终要求：
- 生成高中生能看懂的证明导览，不要声称这是纯高中数学完整严格证明。
- 必须清楚说明反证结构：若存在反例 -> Frey 曲线 -> Ribet 推出非 modular -> Wiles 推出 modular -> 矛盾。
- 标出黑箱定理，并用日常语言解释黑箱的作用。
- 最终只通过 submit_task 提交 JSON，必须符合这个 schema：
{schema}
"#,
        question = run.question,
        plan = serde_json::to_string_pretty(plan).map_err(|error| error.to_string())?,
        outputs = serde_json::to_string_pretty(outputs).map_err(|error| error.to_string())?,
        schema = serde_json::to_string_pretty(&final_result_schema())
            .map_err(|error| error.to_string())?
    ))
}

fn final_result_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["title", "important_boundary", "overview", "sections", "black_box_theorems", "rigor_notes"],
        "properties": {
            "title": { "type": "string" },
            "important_boundary": { "type": "string" },
            "overview": { "type": "string" },
            "sections": {
                "type": "array",
                "minItems": 4,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["heading", "body"],
                    "properties": {
                        "heading": { "type": "string" },
                        "body": { "type": "string" }
                    }
                }
            },
            "black_box_theorems": {
                "type": "array",
                "minItems": 2,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["name", "plain_language"],
                    "properties": {
                        "name": { "type": "string" },
                        "plain_language": { "type": "string" }
                    }
                }
            },
            "rigor_notes": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    })
}

fn fallback_available_agents() -> Vec<AvailableAgent> {
    [
        (
            "elementary-agent",
            "Explains the elementary number theory foundation, reduction ideas, contradiction setup, and the n = 4 descent example in high-school-friendly language.",
        ),
        (
            "bridge-agent",
            "Explains how a hypothetical Fermat counterexample leads to the Frey curve and how Ribet's theorem turns that into a non-modularity claim.",
        ),
        (
            "modularity-agent",
            "Explains modularity, Wiles's theorem, and why the relevant semistable elliptic curve must be modular, using accessible analogies.",
        ),
        (
            "teacher-agent",
            "Rewrites specialist outputs into a coherent high-school-readable proof guide with clear section flow.",
        ),
        (
            "rigor-agent",
            "Checks the guide for mathematical overclaiming, missing black-box labels, and false claims of elementary completeness.",
        ),
    ]
    .into_iter()
    .map(|(name, description)| AvailableAgent {
        name: name.to_owned(),
        description: description.to_owned(),
        runtime: Some("opencode".to_owned()),
        memory: Some("none".to_owned()),
        service_url: format!("http://{name}.svc"),
    })
    .collect()
}

fn default_question() -> String {
    "请用高中生能看懂的方式，解释费马大定理为什么成立。需要展示主线逻辑，但不要伪装成纯初等证明。"
        .to_owned()
}

fn run_from_row(row: &ignis_sdk::sqlite::TypedRow) -> std::result::Result<RunRecord, String> {
    Ok(RunRecord {
        run_id: parse_text(row.values.first(), "run_id")?,
        question: parse_text(row.values.get(1), "question")?,
        status: parse_text(row.values.get(2), "status")?,
        available_agents_json: parse_text(row.values.get(3), "available_agents_json")?,
        child_plan_json: parse_optional_text(row.values.get(4), "child_plan_json")?,
        final_result_json: parse_optional_text(row.values.get(5), "final_result_json")?,
        error_json: parse_optional_text(row.values.get(6), "error_json")?,
    })
}

fn invocation_from_row(
    row: &ignis_sdk::sqlite::TypedRow,
) -> std::result::Result<InvocationRecord, String> {
    Ok(InvocationRecord {
        run_id: parse_text(row.values.first(), "run_id")?,
        plan_task_id: parse_text(row.values.get(1), "plan_task_id")?,
        invocation_kind: parse_text(row.values.get(3), "invocation_kind")?,
    })
}

fn parse_task_state(value: &str) -> std::result::Result<TaskState, String> {
    match value {
        "queued" => Ok(TaskState::Queued),
        "running" => Ok(TaskState::Running),
        "waiting_child_plan" => Ok(TaskState::WaitingChildPlan),
        "succeeded" => Ok(TaskState::Succeeded),
        "failed" => Ok(TaskState::Failed),
        "cancelled" => Ok(TaskState::Cancelled),
        other => Err(format!("unknown task state `{other}`")),
    }
}

fn parse_text(value: Option<&SqliteValue>, field: &str) -> std::result::Result<String, String> {
    match value {
        Some(SqliteValue::Text(value)) => Ok(value.clone()),
        Some(other) => Err(format!("unexpected sqlite type for {field}: {other:?}")),
        None => Err(format!("missing sqlite value for {field}")),
    }
}

fn parse_optional_text(
    value: Option<&SqliteValue>,
    field: &str,
) -> std::result::Result<Option<String>, String> {
    match value {
        Some(SqliteValue::Text(value)) => Ok(Some(value.clone())),
        Some(SqliteValue::Null) => Ok(None),
        Some(other) => Err(format!("unexpected sqlite type for {field}: {other:?}")),
        None => Err(format!("missing sqlite value for {field}")),
    }
}

fn parse_i64(value: Option<&SqliteValue>, field: &str) -> std::result::Result<i64, String> {
    match value {
        Some(SqliteValue::Integer(value)) => Ok(*value),
        Some(other) => Err(format!("unexpected sqlite type for {field}: {other:?}")),
        None => Err(format!("missing sqlite value for {field}")),
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
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
