use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::{Parser, ValueEnum};
use jsonschema::JSONSchema;
use reqwest::Url;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::process::{Child, Command};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "agent-service")]
#[command(about = "Minimal agent task service")]
struct Args {
    #[arg(long, env = "AGENT_SERVICE_CONFIG")]
    config: Option<PathBuf>,

    #[arg(long, env = "AGENT_SERVICE_LISTEN_ADDR")]
    listen_addr: Option<SocketAddr>,

    #[arg(long, env = "AGENT_SERVICE_DATABASE_PATH")]
    database_path: Option<PathBuf>,

    #[arg(long, env = "AGENT_SERVICE_WORKSPACE_DIR")]
    workspace_dir: Option<PathBuf>,

    #[arg(long, env = "AGENT_SERVICE_RUNTIME", value_enum)]
    runtime: Option<AgentRuntime>,

    #[arg(long, env = "AGENT_SERVICE_CODEX_BIN")]
    codex_bin: Option<String>,

    #[arg(long, env = "AGENT_SERVICE_CODEX_MODEL")]
    codex_model: Option<String>,

    #[arg(long, env = "AGENT_SERVICE_OPENCODE_BIN")]
    opencode_bin: Option<String>,

    #[arg(long, env = "AGENT_SERVICE_OPENCODE_MODEL")]
    opencode_model: Option<String>,

    #[arg(long, env = "AGENT_SERVICE_TASK_TIMEOUT_SEC")]
    task_timeout_sec: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct FileConfig {
    listen_addr: Option<SocketAddr>,
    database_path: Option<PathBuf>,
    workspace_dir: Option<PathBuf>,
    runtime: Option<AgentRuntime>,
    codex_bin: Option<String>,
    codex_model: Option<String>,
    opencode_bin: Option<String>,
    opencode_model: Option<String>,
    task_timeout_sec: Option<u64>,
    add_task_bearer_token: Option<String>,
    add_task_bearer_token_env: Option<String>,
    mcp_bearer_token: Option<String>,
    mcp_bearer_token_env: Option<String>,
    callback_host_allowlist: Option<Vec<String>>,
    agents_md_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct Config {
    listen_addr: SocketAddr,
    database_path: PathBuf,
    workspace_dir: PathBuf,
    runtime: AgentRuntime,
    codex_bin: String,
    codex_model: Option<String>,
    opencode_bin: String,
    opencode_model: Option<String>,
    task_timeout_sec: u64,
    add_task_bearer_token: Option<String>,
    mcp_bearer_token: Option<String>,
    callback_host_allowlist: Vec<String>,
    system_prompt: String,
}

impl Config {
    fn load(args: Args) -> Result<Self> {
        let file_config = match args.config.as_deref() {
            Some(path) => {
                let contents = std::fs::read_to_string(path)
                    .with_context(|| format!("failed to read config {}", path.display()))?;
                toml::from_str::<FileConfig>(&contents)
                    .with_context(|| format!("failed to parse config {}", path.display()))?
            }
            None => FileConfig::default(),
        };

        let system_prompt = load_system_prompt(
            file_config
                .agents_md_path
                .unwrap_or_else(|| PathBuf::from("/app/config/AGENTS.md")),
        )?;

        let add_task_bearer_token = resolve_secret(
            file_config.add_task_bearer_token,
            file_config.add_task_bearer_token_env,
        )?
        .or_else(|| env_non_empty("AGENT_SERVICE_ADD_TASK_TOKEN"));
        let mcp_bearer_token = resolve_secret(
            file_config.mcp_bearer_token,
            file_config.mcp_bearer_token_env,
        )?
        .or_else(|| env_non_empty("AGENT_SERVICE_MCP_TOKEN"));
        let callback_host_allowlist = env_list("AGENT_SERVICE_CALLBACK_HOST_ALLOWLIST")
            .or(file_config.callback_host_allowlist)
            .unwrap_or_default();

        Ok(Self {
            listen_addr: args
                .listen_addr
                .or(file_config.listen_addr)
                .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 3900))),
            database_path: args
                .database_path
                .or(file_config.database_path)
                .unwrap_or_else(|| PathBuf::from("./agent-service.sqlite3")),
            workspace_dir: args
                .workspace_dir
                .or(file_config.workspace_dir)
                .unwrap_or_else(|| PathBuf::from("./agent-service-work")),
            runtime: args.runtime.or(file_config.runtime).unwrap_or_default(),
            codex_bin: args
                .codex_bin
                .or(file_config.codex_bin)
                .unwrap_or_else(|| "codex".to_string()),
            codex_model: args.codex_model.or(file_config.codex_model),
            opencode_bin: args
                .opencode_bin
                .or(file_config.opencode_bin)
                .unwrap_or_else(|| "opencode".to_string()),
            opencode_model: args.opencode_model.or(file_config.opencode_model),
            task_timeout_sec: args
                .task_timeout_sec
                .or(file_config.task_timeout_sec)
                .unwrap_or(900),
            add_task_bearer_token,
            mcp_bearer_token,
            callback_host_allowlist,
            system_prompt,
        })
    }
}

fn load_system_prompt(agents_md_path: PathBuf) -> Result<String> {
    let mut system_prompt = default_system_prompt();
    if agents_md_path.exists() {
        let metadata = std::fs::symlink_metadata(&agents_md_path)
            .with_context(|| format!("failed to read {}", agents_md_path.display()))?;
        if metadata.file_type().is_symlink() {
            bail!(
                "agent prompt file cannot be a symlink: {}",
                agents_md_path.display()
            );
        }
        if !metadata.is_file() {
            bail!(
                "agent prompt path must be a file: {}",
                agents_md_path.display()
            );
        }
        let extra = std::fs::read_to_string(&agents_md_path)
            .with_context(|| format!("failed to read {}", agents_md_path.display()))?;
        if !extra.trim().is_empty() {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(extra.trim());
            system_prompt.push('\n');
        }
    }
    Ok(system_prompt)
}

#[derive(Debug, Clone, Copy, Deserialize, Default, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum AgentRuntime {
    #[default]
    Codex,
    Opencode,
}

impl AgentRuntime {
    fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Opencode => "opencode",
        }
    }
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn env_list(name: &str) -> Option<Vec<String>> {
    let values = env_non_empty(name)?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn resolve_secret(value: Option<String>, env_name: Option<String>) -> Result<Option<String>> {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        return Ok(Some(value));
    }
    let Some(env_name) = env_name.filter(|name| !name.trim().is_empty()) else {
        return Ok(None);
    };
    let value = std::env::var(&env_name)
        .with_context(|| format!("failed to read secret from env {env_name}"))?;
    Ok(Some(value))
}

fn default_system_prompt() -> String {
    r#"你是 agent-service 的任务执行 Agent。

你必须遵守以下流程：

1. 先调用 get_task 获取任务。
2. 如果 get_task 返回 task_id = null，说明当前没有任务，直接结束。
3. 如果拿到任务，只处理这一个任务。
4. 不要并行处理多个任务。
5. 不要在 submit_task 成功后继续领取任务。
6. 按任务中的 prompt 完成工作。
7. 最终结果必须符合任务里的 task_result_json_schema。
8. 完成后必须调用 submit_task 提交结果。
9. submit_task 的 result 字段只能放最终 JSON，不要放 Markdown。
10. 如果无法完整完成，也要调用 submit_task，result 按 schema 尽量返回可用信息。
11. submit_task 成功后立即结束本次执行，不要再次调用 get_task。
12. 本次 agent runtime 只服务当前一个任务，不能循环处理队列。

你可以使用浏览器、HTTP、OCR 或其他可用工具获取证据，但不要编造无法确认的信息。"#
        .to_string()
}

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    db: Arc<Mutex<Connection>>,
    runtime: Arc<Mutex<RuntimeState>>,
    http: reqwest::Client,
}

#[derive(Debug, Default)]
struct RuntimeState {
    active_codex_pid: Option<u32>,
    active_codex_claimed_task: bool,
}

#[derive(Debug, Deserialize)]
struct AddTaskRequest {
    prompt: String,
    #[serde(default)]
    callback_url: Option<String>,
    task_result_json_schema: Value,
}

#[derive(Debug, Serialize)]
struct AddTaskResponse {
    task_id: String,
}

#[derive(Debug, Serialize)]
struct TaskStatusResponse {
    task_id: String,
    status: String,
    result: Option<Value>,
    error: Option<Value>,
}

#[derive(Debug)]
struct AgentTask {
    id: String,
    prompt: String,
    callback_url: String,
    task_result_json_schema: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct McpRequest {
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize)]
struct SubmitTaskArgs {
    task_id: String,
    result: Value,
}

#[derive(Debug, Serialize)]
struct ToolError {
    code: String,
    message: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("agent_service=info,tower_http=info")),
        )
        .init();

    let args = Args::parse();
    let config = Arc::new(Config::load(args)?);

    std::fs::create_dir_all(&config.workspace_dir).with_context(|| {
        format!(
            "failed to create workspace dir {}",
            config.workspace_dir.display()
        )
    })?;
    if let Some(parent) = config
        .database_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create database dir {}", parent.display()))?;
    }

    let db = Connection::open(&config.database_path).with_context(|| {
        format!(
            "failed to open sqlite database {}",
            config.database_path.display()
        )
    })?;
    init_db(&db)?;

    let state = AppState {
        config: config.clone(),
        db: Arc::new(Mutex::new(db)),
        runtime: Arc::new(Mutex::new(RuntimeState::default())),
        http: reqwest::Client::new(),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/tasks", post(add_task))
        .route("/v1/tasks/{task_id}", get(get_task_status))
        .route("/mcp", post(mcp))
        .with_state(Arc::new(state));

    info!(
        runtime = config.runtime.as_str(),
        "agent-service listening on {}", config.listen_addr
    );
    let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn init_db(db: &Connection) -> Result<()> {
    db.execute_batch(
        r#"
        create table if not exists agent_tasks (
            id text primary key,
            prompt text not null,
            callback_url text not null,
            task_result_json_schema text not null,
            status text not null,
            result_json text,
            error_json text,
            created_at integer not null,
            started_at integer,
            finished_at integer
        );
        create index if not exists idx_agent_tasks_status_created
            on agent_tasks(status, created_at);
        "#,
    )?;
    Ok(())
}

async fn healthz() -> Json<Value> {
    Json(json!({ "ok": true }))
}

async fn add_task(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<AddTaskRequest>,
) -> Response {
    match add_task_inner(state, headers, payload).await {
        Ok(response) => (StatusCode::ACCEPTED, Json(json!(response))).into_response(),
        Err(error) => error_response(StatusCode::BAD_REQUEST, "INVALID_TASK", error),
    }
}

async fn add_task_inner(
    state: Arc<AppState>,
    headers: HeaderMap,
    payload: AddTaskRequest,
) -> Result<AddTaskResponse> {
    authorize(headers, state.config.add_task_bearer_token.as_deref())?;
    validate_non_empty(&payload.prompt, "prompt")?;
    let callback_url = payload
        .callback_url
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    if !callback_url.is_empty() {
        validate_callback_url(&callback_url, &state.config.callback_host_allowlist)?;
    }
    validate_json_schema(&payload.task_result_json_schema)?;

    let task_id = new_task_id();
    let now = now_sec();
    {
        let db = lock_db(&state)?;
        db.execute(
            r#"
            insert into agent_tasks (
                id,
                prompt,
                callback_url,
                task_result_json_schema,
                status,
                created_at
            )
            values (?1, ?2, ?3, ?4, 'queued', ?5)
            "#,
            params![
                task_id,
                payload.prompt,
                callback_url,
                serde_json::to_string(&payload.task_result_json_schema)?,
                now
            ],
        )?;
    }

    maybe_spawn_codex_exec(state)?;
    Ok(AddTaskResponse { task_id })
}

async fn get_task_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Response {
    if let Err(error) = authorize(headers, state.config.add_task_bearer_token.as_deref()) {
        return error_response(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", error);
    }
    match get_task_status_inner(&state, &task_id) {
        Ok(response) => Json(json!(response)).into_response(),
        Err(error) => error_response(StatusCode::NOT_FOUND, "TASK_NOT_FOUND", error),
    }
}

async fn mcp(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<McpRequest>,
) -> Response {
    if let Err(error) = authorize(headers, state.config.mcp_bearer_token.as_deref()) {
        return error_response(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", error);
    }

    let id = request.id.clone();
    let result = match request.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": request
                .params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2024-11-05"),
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "agent-service",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "tools/list" => Ok(json!({
            "tools": tool_definitions()
        })),
        "tools/call" => handle_tool_call(state, request.params).await,
        "notifications/initialized" => Ok(json!({})),
        _ => Err(anyhow!("unsupported MCP method {}", request.method)),
    };

    let body = match result {
        Ok(result) => json!({
            "jsonrpc": request.jsonrpc.as_deref().unwrap_or("2.0"),
            "id": id,
            "result": result
        }),
        Err(error) => json!({
            "jsonrpc": request.jsonrpc.as_deref().unwrap_or("2.0"),
            "id": id,
            "error": {
                "code": -32000,
                "message": error.to_string()
            }
        }),
    };
    Json(body).into_response()
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "get_task",
            "description": "Get one queued task. This agent must process at most one task and exit after submit_task.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }
        },
        {
            "name": "submit_task",
            "description": "Submit the final JSON result for a running task. The service validates the result with the task JSON schema, stores it, and calls callback_url when the task has one.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["task_id", "result"],
                "properties": {
                    "task_id": { "type": "string" },
                    "result": { "type": "object" }
                }
            }
        }
    ])
}

async fn handle_tool_call(state: Arc<AppState>, params: Value) -> Result<Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("tools/call requires params.name"))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let output = match name {
        "get_task" => tool_get_task(state)?,
        "submit_task" => {
            let args: SubmitTaskArgs = serde_json::from_value(arguments)
                .context("submit_task arguments must match input schema")?;
            tool_submit_task(state, args).await?
        }
        _ => bail!("unknown tool {name}"),
    };

    Ok(json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&output)?
            }
        ],
        "isError": output.get("ok").and_then(Value::as_bool) == Some(false)
    }))
}

fn tool_get_task(state: Arc<AppState>) -> Result<Value> {
    if exists_running_task(&state)? {
        return Ok(json!({ "task_id": null }));
    }

    let task = {
        let db = lock_db(&state)?;
        db.query_row(
            r#"
            select id, prompt, callback_url, task_result_json_schema, status
            from agent_tasks
            where status = 'queued'
            order by created_at asc
            limit 1
            "#,
            [],
            row_to_task,
        )
        .optional()?
    };

    let Some(task) = task else {
        return Ok(json!({ "task_id": null }));
    };

    let now = now_sec();
    {
        let db = lock_db(&state)?;
        db.execute(
            r#"
            update agent_tasks
            set status = 'running', started_at = ?2
            where id = ?1 and status = 'queued'
            "#,
            params![task.id, now],
        )?;
    }
    {
        let mut runtime = lock_runtime(&state)?;
        runtime.active_codex_claimed_task = true;
    }

    info!("codex claimed task {}", task.id);
    Ok(json!({
        "task_id": task.id,
        "prompt": task.prompt,
        "task_result_json_schema": serde_json::from_str::<Value>(&task.task_result_json_schema)?
    }))
}

async fn tool_submit_task(state: Arc<AppState>, args: SubmitTaskArgs) -> Result<Value> {
    let task = get_task_by_id(&state, &args.task_id)?;
    if task.status != "running" {
        return Ok(tool_error("INVALID_TASK_STATE", "task is not running"));
    }

    let schema: Value = serde_json::from_str(&task.task_result_json_schema)?;
    if let Err(error) = validate_json_with_schema(&schema, &args.result) {
        mark_failed(
            &state,
            &task.id,
            "SCHEMA_VALIDATION_FAILED",
            &error.to_string(),
        )?;
        return Ok(tool_error(
            "SCHEMA_VALIDATION_FAILED",
            "result does not match task_result_json_schema",
        ));
    }

    let callback_body = json!({
        "task_id": task.id,
        "status": "succeeded",
        "result": args.result
    });

    if task.callback_url.trim().is_empty() {
        mark_succeeded(&state, &task.id, &callback_body["result"])?;
        info!("task {} succeeded without callback", task.id);
        return Ok(json!({ "ok": true }));
    }

    let callback_response = state
        .http
        .post(&task.callback_url)
        .json(&callback_body)
        .send()
        .await;

    match callback_response {
        Ok(response) if response.status().is_success() => {
            mark_succeeded(&state, &task.id, &callback_body["result"])?;
            info!("task {} succeeded and callback delivered", task.id);
            Ok(json!({ "ok": true }))
        }
        Ok(response) => {
            let status = response.status();
            mark_failed(
                &state,
                &task.id,
                "CALLBACK_FAILED",
                &format!("callback_url returned HTTP {status}"),
            )?;
            Ok(tool_error(
                "CALLBACK_FAILED",
                &format!("callback_url returned HTTP {status}"),
            ))
        }
        Err(error) => {
            mark_failed(&state, &task.id, "CALLBACK_FAILED", &error.to_string())?;
            Ok(tool_error(
                "CALLBACK_FAILED",
                "callback_url returned an error",
            ))
        }
    }
}

fn maybe_spawn_codex_exec(state: Arc<AppState>) -> Result<()> {
    {
        let mut runtime = lock_runtime(&state)?;
        if runtime.active_codex_pid.is_some() {
            return Ok(());
        }
        runtime.active_codex_claimed_task = false;
    }

    if exists_running_task(&state)? || !exists_queued_task(&state)? {
        return Ok(());
    }

    std::fs::create_dir_all(&state.config.workspace_dir).with_context(|| {
        format!(
            "failed to create workspace dir {}",
            state.config.workspace_dir.display()
        )
    })?;
    let prompt_path = state.config.workspace_dir.join("AGENTS.md");
    std::fs::write(&prompt_path, &state.config.system_prompt)
        .with_context(|| format!("failed to write {}", prompt_path.display()))?;

    let mut command = match state.config.runtime {
        AgentRuntime::Codex => {
            let mut command = Command::new(&state.config.codex_bin);
            command
                .arg("exec")
                .arg("--json")
                .arg("--skip-git-repo-check")
                .arg("--color")
                .arg("never")
                .arg("--dangerously-bypass-approvals-and-sandbox");

            if let Some(model) = state.config.codex_model.as_deref() {
                command.arg("--model").arg(model);
            }

            command
                .arg("-C")
                .arg(&state.config.workspace_dir)
                .arg(state.config.system_prompt.clone());
            command
        }
        AgentRuntime::Opencode => {
            let mut command = Command::new(&state.config.opencode_bin);
            command.arg("run").arg("--format").arg("json");
            if let Some(model) = state.config.opencode_model.as_deref() {
                command.arg("--model").arg(model);
            }
            command.arg(state.config.system_prompt.clone());
            command
        }
    };

    command
        .current_dir(&state.config.workspace_dir)
        .kill_on_drop(true);

    let child = command.spawn().with_context(|| {
        format!(
            "failed to spawn {} runtime; ensure it is installed and MCP is configured",
            state.config.runtime.as_str()
        )
    })?;
    let pid = child.id();
    {
        let mut runtime = lock_runtime(&state)?;
        runtime.active_codex_pid = pid;
    }

    info!(
        ?pid,
        runtime = state.config.runtime.as_str(),
        "spawned agent runtime"
    );
    let state_for_wait = state.clone();
    tokio::spawn(async move {
        wait_for_codex(state_for_wait, child).await;
    });

    Ok(())
}

async fn wait_for_codex(state: Arc<AppState>, mut child: Child) {
    let timeout = Duration::from_secs(state.config.task_timeout_sec);
    let wait_result = tokio::time::timeout(timeout, child.wait()).await;

    match wait_result {
        Ok(Ok(status)) => {
            info!(?status, "codex exec exited");
        }
        Ok(Err(error)) => {
            error!(%error, "failed to wait for codex exec");
        }
        Err(_) => {
            warn!(
                "codex exec timed out after {}s",
                state.config.task_timeout_sec
            );
            if let Err(error) = child.start_kill() {
                warn!(%error, "failed to kill timed out codex exec");
            }
            let _ = child.wait().await;
            if let Err(error) = fail_running_task(&state, "CODEX_TIMEOUT", "codex exec timed out") {
                error!(%error, "failed to mark running task as timed out");
            }
        }
    }

    let claimed = {
        match lock_runtime(&state) {
            Ok(mut runtime) => {
                let claimed = runtime.active_codex_claimed_task;
                runtime.active_codex_pid = None;
                runtime.active_codex_claimed_task = false;
                claimed
            }
            Err(error) => {
                error!(%error, "failed to clear codex runtime state");
                false
            }
        }
    };

    if let Err(error) = fail_running_task(
        &state,
        "CODEX_EXITED_WITHOUT_SUBMIT",
        "codex exec exited before submit_task succeeded",
    ) {
        error!(%error, "failed to mark unfinished running task as failed");
    }

    if claimed {
        if let Err(error) = maybe_spawn_codex_exec(state) {
            error!(%error, "failed to spawn next codex exec");
        }
    }
}

fn exists_running_task(state: &AppState) -> Result<bool> {
    let db = lock_db(state)?;
    let count: i64 = db.query_row(
        "select count(*) from agent_tasks where status = 'running'",
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn exists_queued_task(state: &AppState) -> Result<bool> {
    let db = lock_db(state)?;
    let count: i64 = db.query_row(
        "select count(*) from agent_tasks where status = 'queued'",
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn get_task_status_inner(state: &AppState, task_id: &str) -> Result<TaskStatusResponse> {
    let db = lock_db(state)?;
    db.query_row(
        r#"
        select id, status, result_json, error_json
        from agent_tasks
        where id = ?1
        "#,
        params![task_id],
        |row| {
            let result_json: Option<String> = row.get(2)?;
            let error_json: Option<String> = row.get(3)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                result_json,
                error_json,
            ))
        },
    )
    .optional()?
    .map(|(task_id, status, result_json, error_json)| {
        let result = result_json
            .as_deref()
            .and_then(|value| serde_json::from_str::<Value>(value).ok());
        let error = error_json
            .as_deref()
            .and_then(|value| serde_json::from_str::<Value>(value).ok());
        TaskStatusResponse {
            task_id,
            status,
            result,
            error,
        }
    })
    .ok_or_else(|| anyhow!("task not found"))
}

fn get_task_by_id(state: &AppState, task_id: &str) -> Result<AgentTask> {
    let db = lock_db(state)?;
    db.query_row(
        r#"
        select id, prompt, callback_url, task_result_json_schema, status
        from agent_tasks
        where id = ?1
        "#,
        params![task_id],
        row_to_task,
    )
    .optional()?
    .ok_or_else(|| anyhow!("task not found"))
}

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentTask> {
    Ok(AgentTask {
        id: row.get(0)?,
        prompt: row.get(1)?,
        callback_url: row.get(2)?,
        task_result_json_schema: row.get(3)?,
        status: row.get(4)?,
    })
}

fn mark_succeeded(state: &AppState, task_id: &str, result: &Value) -> Result<()> {
    let now = now_sec();
    let db = lock_db(state)?;
    db.execute(
        r#"
        update agent_tasks
        set status = 'succeeded', result_json = ?2, finished_at = ?3
        where id = ?1
        "#,
        params![task_id, serde_json::to_string(result)?, now],
    )?;
    Ok(())
}

fn mark_failed(state: &AppState, task_id: &str, code: &str, message: &str) -> Result<()> {
    let now = now_sec();
    let error_json = json!({
        "code": code,
        "message": message
    });
    let db = lock_db(state)?;
    db.execute(
        r#"
        update agent_tasks
        set status = 'failed', error_json = ?2, finished_at = ?3
        where id = ?1
        "#,
        params![task_id, serde_json::to_string(&error_json)?, now],
    )?;
    Ok(())
}

fn fail_running_task(state: &AppState, code: &str, message: &str) -> Result<()> {
    let task_id = {
        let db = lock_db(state)?;
        db.query_row(
            "select id from agent_tasks where status = 'running' order by started_at asc limit 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    };
    if let Some(task_id) = task_id {
        mark_failed(state, &task_id, code, message)?;
    }
    Ok(())
}

fn validate_non_empty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} is required");
    }
    Ok(())
}

fn validate_json_schema(schema: &Value) -> Result<()> {
    JSONSchema::compile(schema)
        .map(|_| ())
        .map_err(|error| anyhow!("task_result_json_schema is not a valid JSON Schema: {error}"))
}

fn validate_json_with_schema(schema: &Value, value: &Value) -> Result<()> {
    let compiled = JSONSchema::compile(schema).map_err(|error| anyhow!(error.to_string()))?;
    match compiled.validate(value) {
        Ok(()) => Ok(()),
        Err(errors) => {
            let messages = errors
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            bail!("{messages}")
        }
    }
}

fn validate_callback_url(callback_url: &str, allowlist: &[String]) -> Result<()> {
    let url = Url::parse(callback_url).context("callback_url must be a valid URL")?;
    match url.scheme() {
        "http" | "https" => {}
        _ => bail!("callback_url must use http or https"),
    }

    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("callback_url must include a host"))?;

    if host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "169.254.169.254" {
        bail!("callback_url host is not allowed");
    }

    if allowlist.is_empty() {
        return Ok(());
    }

    if allowlist.iter().any(|entry| host_matches(host, entry)) {
        Ok(())
    } else {
        bail!("callback_url host {host} is not in callback_host_allowlist")
    }
}

fn host_matches(host: &str, allow: &str) -> bool {
    if let Some(suffix) = allow.strip_prefix("*.") {
        host.ends_with(&format!(".{suffix}"))
    } else {
        host == allow
    }
}

fn authorize(headers: HeaderMap, token: Option<&str>) -> Result<()> {
    let Some(token) = token else {
        return Ok(());
    };
    let expected = format!("Bearer {token}");
    let actual = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if actual == expected {
        Ok(())
    } else {
        bail!("missing or invalid bearer token")
    }
}

fn tool_error(code: &str, message: &str) -> Value {
    let error = ToolError {
        code: code.to_string(),
        message: message.to_string(),
    };
    json!({
        "ok": false,
        "error": error
    })
}

fn error_response(status: StatusCode, code: &str, error: anyhow::Error) -> Response {
    (
        status,
        Json(json!({
            "error": {
                "code": code,
                "message": error.to_string()
            }
        })),
    )
        .into_response()
}

fn lock_db(state: &AppState) -> Result<std::sync::MutexGuard<'_, Connection>> {
    state
        .db
        .lock()
        .map_err(|_| anyhow!("database mutex poisoned"))
}

fn lock_runtime(state: &AppState) -> Result<std::sync::MutexGuard<'_, RuntimeState>> {
    state
        .runtime
        .lock()
        .map_err(|_| anyhow!("runtime mutex poisoned"))
}

fn new_task_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("task_{nanos:x}")
}

fn now_sec() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
