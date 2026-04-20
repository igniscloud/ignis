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
const ORCHESTRATOR_AGENT: &str = "orchestrator-agent";
const TOOL_CALLBACK_URL: &str = "http://api.svc/internal/taskplan/tools";

#[derive(Debug, Deserialize)]
struct CreateWorkflowRequest {
    #[serde(default)]
    question: Option<String>,
    #[serde(default)]
    audience: Option<String>,
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
        .get("/workflows", |_context: Context| async move {
            if let Err(error) = ensure_schema() {
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error);
            }
            match workflow_history() {
                Ok(history) => json_response(StatusCode::OK, history),
                Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error),
            }
        })
        .expect("register GET /workflows");

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
    let mut question = input
        .question
        .unwrap_or_else(default_question)
        .trim()
        .to_owned();
    if question.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "question is required");
    }
    let audience = input
        .audience
        .unwrap_or_else(|| "high-school".to_owned())
        .trim()
        .to_owned();
    if !audience.is_empty() {
        question.push_str("\n\n目标受众：");
        question.push_str(&audience);
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
    let task_id = create_agent_task(ORCHESTRATOR_AGENT, &prompt, final_result_schema()).await?;
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
        ORCHESTRATOR_AGENT,
        "coordinator_initial",
        "running",
    )?;

    Ok(CreateWorkflowResponse {
        run_id,
        status: "coordinating".to_owned(),
        title: "Math Proof Lab".to_owned(),
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
    let task_id = create_agent_task(ORCHESTRATOR_AGENT, &prompt, final_result_schema()).await?;
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
        ORCHESTRATOR_AGENT,
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
                        service.kind == "agent" && service.service != ORCHESTRATOR_AGENT
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
    let result = run
        .final_result_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<Value>(value).ok());
    let error = run
        .error_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<Value>(value).ok());
    Ok(json!({
        "run_id": run.run_id,
        "title": "Math Proof Lab",
        "question": run.question,
        "status": run.status,
        "available_agents": serde_json::from_str::<Value>(&run.available_agents_json).unwrap_or_else(|_| json!([])),
        "child_plan": run.child_plan_json
            .as_deref()
            .and_then(|value| serde_json::from_str::<Value>(value).ok()),
        "child_tasks": child_tasks,
        "invocations": invocations,
        "agent_outputs": agent_outputs(&run, &result, &error)?,
        "result": result,
        "error": error
    }))
}

fn workflow_history() -> std::result::Result<Value, String> {
    let result = sqlite::query_typed(
        "select run_id, question, status, final_result_json, error_json, created_at_ms, updated_at_ms
         from workflow_runs order by created_at_ms desc limit 30",
        &[] as &[&str],
    )?;
    let mut items = Vec::new();
    for row in &result.rows {
        let question = parse_text(row.values.get(1), "question")?;
        let result_json = parse_optional_text(row.values.get(3), "final_result_json")?;
        let error_json = parse_optional_text(row.values.get(4), "error_json")?;
        items.push(json!({
            "run_id": parse_text(row.values.first(), "run_id")?,
            "question": question,
            "question_preview": preview_text(&question, 96),
            "status": parse_text(row.values.get(2), "status")?,
            "has_result": result_json.is_some(),
            "has_error": error_json.is_some(),
            "created_at_ms": parse_i64(row.values.get(5), "created_at_ms")?,
            "updated_at_ms": parse_i64(row.values.get(6), "updated_at_ms")?
        }));
    }
    Ok(json!({ "history": items }))
}

fn agent_outputs(
    run: &RunRecord,
    result: &Option<Value>,
    error: &Option<Value>,
) -> std::result::Result<Vec<Value>, String> {
    let mut outputs = Vec::new();
    let main_markdown = match result {
        Some(value) => final_result_markdown(value),
        None => {
            let mut body = format!(
                "## 主控 Agent\n\n当前状态：`{}`\n\n主控 Agent 正在规划或合成结果。",
                run.status
            );
            if let Some(error) = error {
                body.push_str("\n\n### Error\n\n");
                body.push_str(&generic_value_markdown(error));
            }
            body
        }
    };
    outputs.push(json!({
        "key": ORCHESTRATOR_AGENT,
        "agent_service": ORCHESTRATOR_AGENT,
        "label": "主控 Agent",
        "kind": "main",
        "state": run.status,
        "markdown": main_markdown,
        "raw": result.clone().or_else(|| error.clone())
    }));

    let rows = sqlite::query_typed(
        "select task_id, agent_service, state, output_json, error_json
         from child_tasks where run_id = ? order by updated_at_ms asc, task_id asc",
        &[run.run_id.as_str()],
    )?;
    for row in &rows.rows {
        let task_id = parse_text(row.values.first(), "task_id")?;
        let agent_service = parse_text(row.values.get(1), "agent_service")?;
        let state = parse_text(row.values.get(2), "state")?;
        let output = parse_optional_text(row.values.get(3), "output_json")?
            .and_then(|value| serde_json::from_str::<Value>(&value).ok());
        let error = parse_optional_text(row.values.get(4), "error_json")?
            .and_then(|value| serde_json::from_str::<Value>(&value).ok());
        let markdown = match output.as_ref() {
            Some(value) => child_output_markdown(&agent_service, &task_id, value),
            None => {
                let mut body = format!(
                    "## {}\n\n任务：`{}`\n\n状态：`{}`",
                    agent_title(&agent_service),
                    task_id,
                    state
                );
                if let Some(error) = error.as_ref() {
                    body.push_str("\n\n### Error\n\n");
                    body.push_str(&generic_value_markdown(error));
                }
                body
            }
        };
        outputs.push(json!({
            "key": format!("{agent_service}:{task_id}"),
            "agent_service": agent_service,
            "task_id": task_id,
            "label": agent_title(&agent_service),
            "kind": "child",
            "state": state,
            "markdown": markdown,
            "raw": output.or(error)
        }));
    }

    Ok(outputs)
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
        r#"你是主 agent，负责规划“Math Proof Lab”的多 agent workflow。

用户目标：
{question}

第一轮硬性要求：
- 你领取 task 后，唯一目标是调用 spawn_task_plan。
- 不要输出自然语言解释，不要调用 submit_task，不要直接结束。
- 如果 Codex 没有把 MCP 工具显示成内建 tool，请用 JSON-RPC 调用 AGENT_SERVICE_MCP_URL。
- 不要用 shell 字符串手写或拼接 JSON；数学表达式里的引号、撇号和反斜杠会破坏 JSON。
- 必须用 python3 的 json.dumps 构造 payload，再用 urllib.request POST 到 AGENT_SERVICE_MCP_URL。
- payload 形状必须是：
  {{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"spawn_task_plan","arguments":{{"task_id": 当前 get_task 返回的 task_id, "task_plan": 你的 TaskPlan}}}}}}
- spawn_task_plan 成功后立即停止本轮。

规划规则：
- 子 TaskPlan 不是固定模板。必须按问题复杂度选择必要 agent，简单问题不要调满所有 agent。
- 简单代数/几何题通常只需要 formal-verifier-agent -> pedagogy-agent。
- 需要外部定理、教材边界或论文来源时，加入 literature-agent。
- 用户指定初中/高中/大学/研究生受众时，通常加入 curriculum-agent。
- 面向用户发布的非平凡证明、黑箱证明或教学翻译，加入 rigor-critic-agent。
- 如果你不确定，创建最小 TaskPlan：formal-verifier-agent -> pedagogy-agent。
- 你创建 pedagogy-agent 任务时，必须要求它写成“费曼式深入浅出”：先直觉、再定义、再例子、再一般证明，并解释每一步为什么自然。
- 你最终合成时也必须保持这种风格；不要只罗列证明步骤。

可用子 agent：
{agents}

TaskPlan 必须是 JSON，字段为：
- id
- root_task_id
- tasks: 每个 task 包含 id, agent_service, prompt, input, output_schema
- dependencies: 可为空；需要串联时使用 from_task, to_task, bindings

可用任务设计模式：
- trivial/direct：formal-verifier-agent -> pedagogy-agent
- audience-sensitive：formal-verifier-agent -> curriculum-agent -> pedagogy-agent -> rigor-critic-agent
- literature-heavy：literature-agent -> formal-verifier-agent -> curriculum-agent -> pedagogy-agent -> rigor-critic-agent
- unrestricted/research：literature-agent -> formal-verifier-agent -> rigor-critic-agent，必要时再调用 pedagogy-agent 做渲染整理

最小 TaskPlan 示例，简单问题可直接使用：
{{
  "id": "minimal-proof-workflow",
  "root_task_id": "pedagogy-draft",
  "tasks": [
    {{
      "id": "formal-proof",
      "agent_service": "formal-verifier-agent",
      "prompt": "Prove or verify the claim. Return proof status, key steps, assumptions, and any gaps. Do not overclaim formal verification.",
      "input": {{}},
      "output_schema": {{"type":"object","required":["markdown"],"properties":{{"markdown":{{"type":"string"}}}},"additionalProperties":true}}
    }},
    {{
      "id": "pedagogy-draft",
      "agent_service": "pedagogy-agent",
      "prompt": "Translate the verified proof into the requested audience level in a Feynman-style explanation: start from intuition, give a tiny concrete example, then define the exact terms, then prove the general case step by step. Explain why each step is natural. Preserve proof status and assumptions.",
      "input": {{}},
      "output_schema": {{"type":"object","required":["markdown"],"properties":{{"markdown":{{"type":"string"}}}},"additionalProperties":true}}
    }}
  ],
  "dependencies": [
    {{"from_task":"formal-proof","to_task":"pedagogy-draft","bindings":[{{"from_pointer":"","to_pointer":"/formal_proof"}}]}}
  ]
}}

复杂定理参考 TaskPlan，可按需删减：
{{
  "id": "math-proof-workflow",
  "root_task_id": "rigor-review",
  "tasks": [
    {{
      "id": "literature-knowledge",
      "agent_service": "literature-agent",
      "prompt": "Retrieve authoritative papers, textbooks, curriculum boundaries, and formal-library references relevant to the theorem. Include textbook-level knowledge boundaries and source-backed theorem statements.",
      "input": {{}},
      "output_schema": {{"type":"object","additionalProperties":false,"required":["summary","sources","curriculum_evidence","missing_sources"],"properties":{{"summary":{{"type":"string"}},"sources":{{"type":"array","items":{{"type":"object","additionalProperties":false,"required":["title","supports"],"properties":{{"title":{{"type":"string"}},"supports":{{"type":"string"}},"url":{{"type":"string"}}}}}}}},"curriculum_evidence":{{"type":"array","items":{{"type":"string"}}}},"missing_sources":{{"type":"array","items":{{"type":"string"}}}}}}}}
    }},
    {{
      "id": "formal-dependency",
      "agent_service": "formal-verifier-agent",
      "prompt": "Build a strict proof dependency tree from the literature evidence. Mark each node as machine_checked, literature_checked, black_box, proof_sketch_only, or unavailable. Do not fabricate Lean/Coq/Isabelle results.",
      "input": {{}},
      "output_schema": {{"type":"object","additionalProperties":false,"required":["summary","dependency_tree","black_box_ledger","formal_gaps"],"properties":{{"summary":{{"type":"string"}},"dependency_tree":{{"type":"array","items":{{"type":"object","additionalProperties":false,"required":["id","claim","status"],"properties":{{"id":{{"type":"string"}},"claim":{{"type":"string"}},"status":{{"type":"string"}},"depends_on":{{"type":"array","items":{{"type":"string"}}}}}}}}}},"black_box_ledger":{{"type":"array","items":{{"type":"string"}}}},"formal_gaps":{{"type":"array","items":{{"type":"string"}}}}}}}}
    }},
    {{
      "id": "curriculum-diff",
      "agent_service": "curriculum-agent",
      "prompt": "Compare the proof dependency tree against the requested audience level. Report known knowledge, missing prerequisites, teach-now modules, and black boxes. If audience is unrestricted, skip educational simplification and keep research-level dependencies.",
      "input": {{}},
      "output_schema": {{"type":"object","additionalProperties":false,"required":["audience_assumptions","missing_prerequisites","bridge_modules","black_boxes"],"properties":{{"audience_assumptions":{{"type":"array","items":{{"type":"string"}}}},"missing_prerequisites":{{"type":"array","items":{{"type":"string"}}}},"bridge_modules":{{"type":"array","items":{{"type":"string"}}}},"black_boxes":{{"type":"array","items":{{"type":"string"}}}}}}}}
    }},
    {{
      "id": "pedagogy-draft",
      "agent_service": "pedagogy-agent",
      "prompt": "Translate the strict proof dependency tree into the requested audience level. Include knowledge expansion modules before using missing concepts. Label known, teach_now, black_box, and intuition-only steps.",
      "input": {{}},
      "output_schema": {{"type":"object","additionalProperties":false,"required":["draft_title","sections","black_box_theorems","step_annotations"],"properties":{{"draft_title":{{"type":"string"}},"sections":{{"type":"array","items":{{"type":"object","additionalProperties":false,"required":["heading","body"],"properties":{{"heading":{{"type":"string"}},"body":{{"type":"string"}}}}}}}},"black_box_theorems":{{"type":"array","items":{{"type":"object","additionalProperties":false,"required":["name","plain_language"],"properties":{{"name":{{"type":"string"}},"plain_language":{{"type":"string"}}}}}}}},"step_annotations":{{"type":"array","items":{{"type":"string"}}}}}}}}
    }},
    {{
      "id": "rigor-review",
      "agent_service": "rigor-critic-agent",
      "prompt": "Review the pedagogical draft against the formal dependency tree. Reject overclaiming, hidden black boxes, missing conditions, analogy-as-proof, or proof-status drift. Return required fixes.",
      "input": {{}},
      "output_schema": {{"type":"object","additionalProperties":false,"required":["approved","rigor_notes","required_fixes"],"properties":{{"approved":{{"type":"boolean"}},"rigor_notes":{{"type":"array","items":{{"type":"string"}}}},"required_fixes":{{"type":"array","items":{{"type":"string"}}}}}}}}
    }}
  ],
  "dependencies": [
    {{"from_task":"literature-knowledge","to_task":"formal-dependency","bindings":[{{"from_pointer":"","to_pointer":"/literature"}}]}},
    {{"from_task":"formal-dependency","to_task":"curriculum-diff","bindings":[{{"from_pointer":"","to_pointer":"/formal_dependency_tree"}}]}},
    {{"from_task":"formal-dependency","to_task":"pedagogy-draft","bindings":[{{"from_pointer":"","to_pointer":"/formal_dependency_tree"}}]}},
    {{"from_task":"curriculum-diff","to_task":"pedagogy-draft","bindings":[{{"from_pointer":"","to_pointer":"/curriculum_diff"}}]}},
    {{"from_task":"pedagogy-draft","to_task":"rigor-review","bindings":[{{"from_pointer":"","to_pointer":"/pedagogy_draft"}}]}},
    {{"from_task":"formal-dependency","to_task":"rigor-review","bindings":[{{"from_pointer":"","to_pointer":"/formal_dependency_tree"}}]}}
  ]
}}
"#,
        question = question,
        agents = serde_json::to_string_pretty(available_agents).unwrap_or_else(|_| "[]".to_owned())
    )
}

fn child_task_prompt(run: &RunRecord, task: &taskplan::TaskSpec, input: &Value) -> String {
    format!(
        r#"你是 `{agent}`，正在执行Math Proof Lab workflow 的子任务。

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
        r#"你是主 agent。子 TaskPlan 已经完成，现在请合成最终的“Math Proof Lab”。

用户目标：
{question}

子 TaskPlan：
{plan}

子 agent 输出：
{outputs}

最终要求：
- 生成符合用户受众要求的数学证明说明；如果用户要求初中/高中/大学/研究生层级，必须保留知识缺口和黑箱标注。
- 如果用户选择不受限制，优先给出最直接、最严格、最完整的证明路径和依赖树摘要。
- 必须清楚区分 machine_checked、literature_checked、black_box、proof_sketch_only 和 intuition-only。
- 最终 Markdown 必须像费曼讲解：从一个直观问题或小例子开始，解释为什么这个定义/变形自然，再逐步抽象为严格证明。
- 每个关键步骤都回答“这一步想解决什么问题？”和“为什么它是合法的？”。
- 保持短段落和清晰小标题，让读者可以从直觉一路跟到严格证明。
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
        "required": ["title", "important_boundary", "overview", "sections", "black_box_theorems", "rigor_notes", "markdown"],
        "properties": {
            "title": { "type": "string" },
            "important_boundary": { "type": "string" },
            "overview": { "type": "string" },
            "markdown": { "type": "string" },
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
            "literature-agent",
            "Retrieves authoritative papers, textbooks, curriculum boundaries, knowledge graph edges, and formal-library references.",
        ),
        (
            "formal-verifier-agent",
            "Interacts with Lean/Coq/Isabelle where possible and emits a strict proof dependency tree with verification status.",
        ),
        (
            "curriculum-agent",
            "Maps the proof dependency tree against middle-school, high-school, undergraduate, graduate, or unrestricted audiences.",
        ),
        (
            "pedagogy-agent",
            "Translates strict proof dependencies into an audience-level explanation while preserving proof status labels.",
        ),
        (
            "rigor-critic-agent",
            "Reviews the explanation against the formal dependency tree and rejects missing conditions, hidden black boxes, or analogy-as-proof.",
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
    "用高中生能看懂的方式说明费马大定理为什么成立；不能把高等数学黑箱伪装成初等证明。".to_owned()
}

fn agent_title(agent_service: &str) -> &'static str {
    match agent_service {
        "orchestrator-agent" => "主控 Agent",
        "literature-agent" => "文献与知识图谱 Agent",
        "formal-verifier-agent" => "形式化逻辑 Agent",
        "curriculum-agent" => "认知水位 Agent",
        "pedagogy-agent" => "教学翻译 Agent",
        "rigor-critic-agent" => "严谨性审稿 Agent",
        _ => "Agent",
    }
}

fn final_result_markdown(value: &Value) -> String {
    if let Some(markdown) = value.get("markdown").and_then(Value::as_str) {
        return markdown.to_owned();
    }

    let mut out = String::new();
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Math Proof Lab Result");
    out.push_str("# ");
    out.push_str(title);
    out.push_str("\n\n");

    if let Some(boundary) = value.get("important_boundary").and_then(Value::as_str) {
        out.push_str("> ");
        out.push_str(boundary);
        out.push_str("\n\n");
    }
    if let Some(overview) = value.get("overview").and_then(Value::as_str) {
        out.push_str("## 概览\n\n");
        out.push_str(overview);
        out.push_str("\n\n");
    }
    if let Some(sections) = value.get("sections").and_then(Value::as_array) {
        for section in sections {
            if let Some(heading) = section.get("heading").and_then(Value::as_str) {
                out.push_str("## ");
                out.push_str(heading);
                out.push_str("\n\n");
            }
            if let Some(body) = section.get("body").and_then(Value::as_str) {
                out.push_str(body);
                out.push_str("\n\n");
            }
        }
    }
    if let Some(black_boxes) = value.get("black_box_theorems").and_then(Value::as_array) {
        out.push_str("## 黑箱与依赖\n\n");
        if black_boxes.is_empty() {
            out.push_str("- 无需高等数学黑箱。\n\n");
        } else {
            for item in black_boxes {
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Unnamed");
                let plain = item
                    .get("plain_language")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                out.push_str("- **");
                out.push_str(name);
                out.push_str("**：");
                out.push_str(plain);
                out.push('\n');
            }
            out.push('\n');
        }
    }
    if let Some(notes) = value.get("rigor_notes").and_then(Value::as_array) {
        out.push_str("## 严谨性说明\n\n");
        for note in notes {
            if let Some(note) = note.as_str() {
                out.push_str("- ");
                out.push_str(note);
                out.push('\n');
            }
        }
        out.push('\n');
    }
    if out.trim().is_empty() {
        generic_value_markdown(value)
    } else {
        out
    }
}

fn child_output_markdown(agent_service: &str, task_id: &str, value: &Value) -> String {
    if let Some(markdown) = value.get("markdown").and_then(Value::as_str) {
        return markdown.to_owned();
    }
    let mut out = format!(
        "## {}\n\n任务：`{}`\n\n",
        agent_title(agent_service),
        task_id
    );
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                out.push_str("### ");
                out.push_str(&title_case_key(key));
                out.push_str("\n\n");
                out.push_str(&value_to_markdown(value));
                out.push_str("\n\n");
            }
        }
        _ => out.push_str(&value_to_markdown(value)),
    }
    out
}

fn value_to_markdown(value: &Value) -> String {
    match value {
        Value::String(value) => value.to_owned(),
        Value::Array(values) => {
            let mut out = String::new();
            for item in values {
                out.push_str("- ");
                match item {
                    Value::String(text) => out.push_str(text),
                    _ => out.push_str(&inline_json(item)),
                }
                out.push('\n');
            }
            out
        }
        Value::Object(map) => {
            let mut out = String::new();
            for (key, value) in map {
                out.push_str("- **");
                out.push_str(&title_case_key(key));
                out.push_str("**: ");
                match value {
                    Value::String(text) => out.push_str(text),
                    _ => out.push_str(&inline_json(value)),
                }
                out.push('\n');
            }
            out
        }
        _ => inline_json(value),
    }
}

fn generic_value_markdown(value: &Value) -> String {
    format!(
        "```json\n{}\n```",
        serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_owned())
    )
}

fn inline_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

fn title_case_key(key: &str) -> String {
    key.replace('_', " ")
        .split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut out = first.to_uppercase().collect::<String>();
                    out.push_str(chars.as_str());
                    out
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn preview_text(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::new();
    for ch in compact.chars().take(max_chars) {
        out.push(ch);
    }
    if compact.chars().count() > max_chars {
        out.push('…');
    }
    out
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
