# Agent Service 极简实现方案

本文定义一个最小可用的 `agent-service`。

目标不是做复杂任务平台，而是提供一个很薄的服务：

1. 来源 service 提交一个 Agent 任务
2. `agent-service` 返回 `task_id`
3. `agent-service` 为这个任务启动一次 `codex exec`
4. Codex 通过 `get_task` 工具领取任务
5. Codex 完成任务后调用 `submit_task`
6. `submit_task` 校验结果 JSON Schema
7. 校验通过后，`agent-service` 调用任务里的 `callback_url`

第一版只允许同一时间处理一个任务，不做并行。每个任务都启动一次新的 `codex exec`，任务完成后该 Codex 进程退出。

## 1. 核心原则

保持简单：

- `add_task` 请求只有 3 个业务字段
- 不需要 `schema_ref`
- 不需要预定义任务类型
- 不需要复杂 options
- 不需要 source metadata
- 不需要 callback registry
- 不需要 worker pool
- 同一时间只处理一个任务
- 每个任务启动一次 `codex exec`
- `codex exec` 允许全部权限，由外层 Podman/容器负责隔离
- Codex 通过工具自己领取任务和提交任务

`agent-service` 只负责：

- 存任务
- 启动一次性 Codex 执行进程
- 提供 `get_task`
- 提供 `submit_task`
- 校验 `task_result_json_schema`
- 调用 `callback_url`
- 记录任务状态

## 2. 对外 API

### 2.1 添加任务

来源 service 调用：

```http
POST /v1/tasks
Content-Type: application/json
```

请求体只有 3 个字段：

```json
{
  "prompt": "请访问这个小红书链接，拆解情绪价值、关键词、结构框架，并生成可复用模板：http://xhslink.com/o/5RZBGIk4WhR",
  "callback_url": "http://source-service.internal/api/agent-callback",
  "task_result_json_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": [
      "summary",
      "emotion_value",
      "keywords",
      "structure",
      "templates",
      "evidence",
      "risk_notes"
    ],
    "properties": {
      "summary": {
        "type": "string"
      },
      "emotion_value": {
        "type": "array",
        "items": {
          "type": "object",
          "required": ["label", "description"],
          "properties": {
            "label": { "type": "string" },
            "description": { "type": "string" }
          }
        }
      },
      "keywords": {
        "type": "array",
        "items": { "type": "string" }
      },
      "structure": {
        "type": "array",
        "items": {
          "type": "object",
          "required": ["step", "name", "description"],
          "properties": {
            "step": { "type": "integer" },
            "name": { "type": "string" },
            "description": { "type": "string" }
          }
        }
      },
      "templates": {
        "type": "array",
        "items": {
          "type": "object",
          "required": ["title_template", "outline"],
          "properties": {
            "title_template": { "type": "string" },
            "outline": {
              "type": "array",
              "items": { "type": "string" }
            }
          }
        }
      },
      "evidence": {
        "type": "array",
        "items": {
          "type": "object",
          "required": ["type", "text"],
          "properties": {
            "type": { "type": "string" },
            "text": { "type": "string" }
          }
        }
      },
      "risk_notes": {
        "type": "array",
        "items": { "type": "string" }
      }
    }
  }
}
```

响应：

```http
202 Accepted
Content-Type: application/json
```

```json
{
  "task_id": "task_01HW0000000000000000000000"
}
```

`agent-service` 收到请求后只做三件事：

1. 校验 `prompt`、`callback_url`、`task_result_json_schema` 存在
2. 生成 `task_id`
3. 保存任务为 `queued`
4. 如果当前没有正在运行的任务，启动一次 `codex exec` 处理队列中的第一个任务

## 3. 内部状态

第一版任务状态只需要：

```text
queued
running
succeeded
failed
```

状态含义：

- `queued`：任务已创建，等待 Codex 获取
- `running`：Codex 已通过 `get_task` 领取任务
- `succeeded`：`submit_task` 校验 schema 成功，并且 callback 成功
- `failed`：agent runtime 超时、退出时仍未成功提交，或 callback 失败。schema 校验失败只返回工具错误，任务保持 `running` 以便 agent 修正重试。

第一版不做并发，所以只需要保证：

```text
同一时间最多一个 task 处于 running
同一时间最多一个 codex exec 进程
```

任务执行模型：

```text
add_task
  -> insert queued task
  -> spawn codex exec
  -> Codex calls get_task
  -> task becomes running
  -> Codex calls submit_task
  -> schema validation
  -> callback_url
  -> task becomes succeeded or failed
  -> codex exec exits
```

## 4. Codex 工具

`agent-service` 需要给 Codex 暴露两个工具：

- `get_task`
- `submit_task`

这两个工具可以通过 MCP 暴露，也可以通过本地 wrapper 命令暴露。推荐第一版用 MCP。

### 4.1 `get_task`

用途：让 Codex 获取一个待处理任务。

输入：

```json
{}
```

输出：

如果有任务：

```json
{
  "task_id": "task_01HW0000000000000000000000",
  "prompt": "请访问这个小红书链接，拆解情绪价值、关键词、结构框架，并生成可复用模板：http://xhslink.com/o/5RZBGIk4WhR",
  "task_result_json_schema": {
    "type": "object",
    "required": ["summary"],
    "properties": {
      "summary": {
        "type": "string"
      }
    }
  }
}
```

如果没有任务：

```json
{
  "task_id": null
}
```

`get_task` 的服务端逻辑：

```text
1. 如果已有 running 任务，返回 task_id = null
2. 查找最早的 queued 任务
3. 如果没有 queued 任务，返回 task_id = null
4. 将该任务标记为 running
5. 返回 task_id、prompt、task_result_json_schema
```

### 4.2 `submit_task`

用途：让 Codex 提交任务结果。

输入：

```json
{
  "task_id": "task_01HW0000000000000000000000",
  "result": {
    "summary": "这是一条通过反常识问题制造停留的小红书图文笔记。",
    "emotion_value": [
      {
        "label": "共情与反差",
        "description": "用动物困境映射人的压力和无力感。"
      }
    ],
    "keywords": ["抑郁", "猪圈", "社会压力"],
    "structure": [
      {
        "step": 1,
        "name": "反常识问题",
        "description": "标题提出违反直觉的问题，吸引用户继续看。"
      }
    ],
    "templates": [
      {
        "title_template": "{对象}一直处在{限制环境}里，为什么不会{情绪问题}？",
        "outline": [
          "提出反常识问题",
          "列出异常表现",
          "给出情绪解释",
          "生成可复用模板"
        ]
      }
    ],
    "evidence": [
      {
        "type": "page",
        "text": "页面标题为：猪一生都被关在猪圈，但是为什么不会抑郁？"
      }
    ],
    "risk_notes": [
      "原内容疑似搬运截图，不建议直接复制。"
    ]
  }
}
```

输出：

成功：

```json
{
  "ok": true
}
```

失败：

```json
{
  "ok": false,
  "error": {
    "code": "SCHEMA_VALIDATION_FAILED",
    "message": "result does not match task_result_json_schema: <validation details>"
  }
}
```

`submit_task` 的服务端逻辑：

```text
1. 根据 task_id 读取任务
2. 确认任务状态是 running
3. 使用 task_result_json_schema 校验 result
4. 校验失败：
   - 不关闭任务，不把任务标记为 failed
   - 返回 ok = false，并带上具体校验错误
   - MCP tool result 标记 isError = true，让 agent 修正 result 后继续调用 submit_task
   - 任务保持 running，直到 submit_task 成功或 agent runtime 超时
5. 校验成功：
   - 调用 callback_url
   - callback 成功后，任务标记为 succeeded
   - callback 失败后，任务标记为 failed
   - 返回 ok = true 或 ok = false
```

## 5. Callback 请求

`submit_task` 校验成功后，`agent-service` 调用任务中的 `callback_url`。

请求：

```http
POST {callback_url}
Content-Type: application/json
```

Body：

```json
{
  "task_id": "task_01HW0000000000000000000000",
  "status": "succeeded",
  "result": {
    "summary": "这是一条通过反常识问题制造停留的小红书图文笔记。",
    "emotion_value": [],
    "keywords": [],
    "structure": [],
    "templates": [],
    "evidence": [],
    "risk_notes": []
  }
}
```

schema 校验失败不会触发失败 callback，也不会结束任务；它只作为 `submit_task` 工具调用错误返回给 agent，让 agent 修正后重试。callback 失败才会结束任务并标记为 failed。

如果要做失败 callback，格式：

```json
{
  "task_id": "task_01HW0000000000000000000000",
  "status": "failed",
  "result": null,
  "error": {
    "code": "SCHEMA_VALIDATION_FAILED",
    "message": "result does not match task_result_json_schema"
  }
}
```

## 6. Codex 系统提示词

每个任务启动一次 `codex exec`。启动 Codex 时，需要增加系统提示词，约束它只处理一个任务，并且必须通过工具提交。

示例：

```text
你是 agent-service 的任务执行 Agent。

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
12. 本次 codex exec 只服务当前一个任务，不能循环处理队列。

你可以使用浏览器、HTTP、OCR 或其他可用工具获取证据，但不要编造无法确认的信息。
```

## 7. Codex 运行方式

第一版不是常驻 Codex。`agent-service` 每处理一个任务，就启动一次 `codex exec`。这和 `agenticcode_worker` 的做法一致：后端负责控制一次执行，Codex 作为一次性 agent runtime 运行。

参考 `agenticcode/backend/crates/agenticcode_worker/src/agent_runtime/codex.rs`，现有 worker 使用：

```text
codex exec --dangerously-bypass-approvals-and-sandbox
```

因此这里也直接使用全权限模式。安全边界放在外层 Podman/容器，而不是 Codex 沙箱。

示例：

```bash
codex exec \
  --json \
  --skip-git-repo-check \
  --color never \
  --dangerously-bypass-approvals-and-sandbox \
  -C /work/agent-service \
  "$(cat /work/agent-service/AGENTS.md)"
```

如果需要读取最终消息，增加：

```bash
--output-last-message /work/agent-service/runs/{task_id}/last-message.txt
```

由于任务是通过 `get_task` 获取的，启动 prompt 不需要包含具体任务内容。prompt 只需要包含第 6 节的系统约束。

`agent-service` 启动 Codex 的规则：

```text
1. add_task 保存 queued 任务
2. 如果已有 running task 或 active codex process，不启动新进程
3. 如果没有 active codex process，启动一次 codex exec
4. Codex 通过 get_task 领取一个 queued task
5. submit_task 后进程结束
6. 如果队列里还有任务，下一次调度再启动新的 codex exec
```

Codex 的 MCP 配置需要包含 `get_task` 和 `submit_task`：

```bash
codex mcp add agent-service \
  --url http://127.0.0.1:3900/mcp
```

## 8. Service 配置

crate 名称：

```text
agent-service
```

示例配置文件：

```text
config/agent-service.example.toml
```

最小配置：

```toml
listen_addr = "127.0.0.1:3900"
database_path = "./agent-service.sqlite3"
workspace_dir = "./agent-service-work"

codex_bin = "codex"
task_timeout_sec = 900

# codex_model = "gpt-5.4-mini"

callback_host_allowlist = [
  "*.internal",
  "*.service.local",
]
```

配置项：

- `listen_addr`：HTTP 服务监听地址
- `database_path`：SQLite 数据库路径
- `workspace_dir`：`codex exec` 的工作目录
- `codex_bin`：Codex CLI 路径，默认 `codex`
- `codex_model`：可选，传给 `codex exec --model`
- `task_timeout_sec`：单任务超时时间
- `add_task_bearer_token_env`：可选，`POST /v1/tasks` 鉴权 token 的环境变量名
- `mcp_bearer_token_env`：可选，`POST /mcp` 鉴权 token 的环境变量名
- `callback_host_allowlist`：callback host allowlist，支持 `*.internal` 这种后缀匹配
- `agents_md_path`：可选，agent 角色说明文件路径，默认 `/app/config/AGENTS.md`。如果文件存在，`agent-service` 会把它追加到内置 one-task 系统提示词后面。

运行：

```bash
cargo run -p agent-service -- \
  --config config/agent-service.example.toml
```

构建：

```bash
cargo build -p agent-service
```

## 9. 最小数据库表

第一版一张表即可：

```sql
create table agent_tasks (
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
```

状态约束可以在应用层处理。

## 10. HTTP 路由

最小路由：

```text
POST /v1/tasks
POST /mcp
GET  /healthz
```

`/mcp` 暴露两个工具：

```text
get_task
submit_task
```

如果暂时不接 MCP，也可以先做普通 HTTP 内部接口：

```text
POST /internal/tools/get_task
POST /internal/tools/submit_task
```

然后用本地 wrapper 命令把它们包装成 Codex 可调用工具。

## 11. 最小实现伪代码

### 11.1 `add_task`

```rust
async fn add_task(req: AddTaskRequest) -> Result<AddTaskResponse> {
    validate_non_empty(req.prompt)?;
    validate_callback_url(&req.callback_url)?;
    validate_json_schema(&req.task_result_json_schema)?;

    let task_id = new_task_id();

    db.insert_task(AgentTask {
        id: task_id.clone(),
        prompt: req.prompt,
        callback_url: req.callback_url,
        task_result_json_schema: serde_json::to_string(&req.task_result_json_schema)?,
        status: "queued".to_string(),
        result_json: None,
        error_json: None,
    }).await?;

    maybe_spawn_codex_exec().await?;

    Ok(AddTaskResponse { task_id })
}
```

### 11.2 `maybe_spawn_codex_exec`

```rust
async fn maybe_spawn_codex_exec() -> Result<()> {
    if db.exists_running_task().await? {
        return Ok(());
    }

    if process_registry.has_active_codex().await? {
        return Ok(());
    }

    if !db.exists_queued_task().await? {
        return Ok(());
    }

    let mut cmd = tokio::process::Command::new("codex");
    cmd.arg("exec")
        .arg("--json")
        .arg("--skip-git-repo-check")
        .arg("--color")
        .arg("never")
        .arg("--dangerously-bypass-approvals-and-sandbox")
        .arg("-C")
        .arg("/work/agent-service")
        .arg(read_system_prompt()?);

    let child = cmd.spawn()?;
    process_registry.record_codex_pid(child.id()).await?;

    tokio::spawn(async move {
        let _ = wait_and_record_codex_exit(child).await;
    });

    Ok(())
}
```

### 11.3 `get_task`

```rust
async fn get_task() -> Result<GetTaskResponse> {
    if db.exists_running_task().await? {
        return Ok(GetTaskResponse { task_id: None });
    }

    let Some(task) = db.first_queued_task().await? else {
        return Ok(GetTaskResponse { task_id: None });
    };

    db.mark_running(&task.id).await?;

    Ok(GetTaskResponse {
        task_id: Some(task.id),
        prompt: Some(task.prompt),
        task_result_json_schema: Some(task.task_result_json_schema),
    })
}
```

### 11.4 `submit_task`

```rust
async fn submit_task(req: SubmitTaskRequest) -> Result<SubmitTaskResponse> {
    let task = db.get_task(&req.task_id).await?;

    if task.status != "running" {
        return Ok(SubmitTaskResponse::error(
            "INVALID_TASK_STATE",
            "task is not running",
        ));
    }

    let schema = parse_schema(&task.task_result_json_schema)?;

    if let Err(err) = validate_json(&schema, &req.result) {
        return Ok(SubmitTaskResponse::error(
            "SCHEMA_VALIDATION_FAILED",
            &format!("result does not match task_result_json_schema: {err}"),
        ));
    }

    let callback_body = serde_json::json!({
        "task_id": task.id,
        "status": "succeeded",
        "result": req.result
    });

    if let Err(err) = http.post_json(&task.callback_url, callback_body).await {
        db.mark_failed(&task.id, "CALLBACK_FAILED", &err).await?;
        return Ok(SubmitTaskResponse::error(
            "CALLBACK_FAILED",
            "callback_url returned an error",
        ));
    }

    db.mark_succeeded(&task.id, &req.result).await?;

    Ok(SubmitTaskResponse { ok: true, error: None })
}
```

## 12. 安全边界

Codex 使用：

```text
--dangerously-bypass-approvals-and-sandbox
```

这意味着 Codex 在容器内部有完整执行权限。这个选择是有意的：让 Agent 可以自由运行浏览器、curl、脚本、OCR、文件处理等工具，减少“工具可用但权限不够”的失败。

因此安全边界必须放在外层：

- `callback_url` 只允许内部域名或显式 allowlist
- `task_result_json_schema` 限制最大大小，例如 64KB
- `prompt` 限制最大大小，例如 64KB
- `submit_task.result` 限制最大大小，例如 1MB
- `get_task` 和 `submit_task` 需要内部 token
- Codex 必须运行在 Podman 或独立容器里
- 不挂载宿主机敏感目录
- 不挂载宿主机 Podman/Docker socket
- 容器网络按需要限制出口
- 每个任务有超时，超时后杀掉 Codex 进程并标记 failed

第一版可以不做复杂权限模型，但不能允许任意外部用户调用 `get_task` 或 `submit_task`。

## 13. 为什么这样设计

这个版本的设计重点是快：

- source service 不需要理解 Agent 平台细节
- request 只有 3 个字段
- schema 由调用方直接给
- Codex 自己通过工具领取和提交任务
- `submit_task` 负责 schema 校验和 callback
- 每个任务启动一次 `codex exec`，执行边界清楚
- Codex 使用全权限，减少工具执行失败
- 同一时间只处理一个任务，避免锁、并发和调度复杂度

后续如果需要再加能力，可以逐步增加：

- callback 重试
- 任务超时
- 失败 callback
- 任务日志
- artifacts
- 多 worker 并发
- idempotency key
- callback 签名

但这些都不是第一版必须项。

## 14. 云端部署

服务名：

```text
agent-service
```

部署目录：

```text
deploy/agent-service/
```

目录内容：

```text
Containerfile
build_image.sh
compose.yaml
agent-service.toml
entrypoint.sh
```

### 14.1 镜像内容

运行镜像包含：

- `agent-service` 二进制
- Codex CLI
- Node.js
- Playwright
- Chromium
- `curl`、`git`、`python3`

镜像启动时，`entrypoint.sh` 会先为 Codex 注册 MCP：

```bash
codex mcp add agent-service \
  --url http://127.0.0.1:3900/mcp
```

然后启动：

```bash
agent-service \
  --config /app/config/agent-service.toml \
  --listen-addr 0.0.0.0:3900
```

Codex 进程和 service 在同一个容器内，所以 Codex 调用 MCP 使用：

```text
http://127.0.0.1:3900/mcp
```

### 14.2 构建镜像

在云端机器或 CI 里执行：

```bash
cd /home/hy/workplace/ignis/deploy/agent-service
./build_image.sh
```

脚本会做两件事：

```text
1. cargo build -p agent-service --release
2. podman build -t ghcr.io/igniscloud/agents/agent-service:latest
```

发布到平台 registry：

```bash
podman push ghcr.io/igniscloud/agents/agent-service:latest
```

当前对外产品不支持用户自定义 agent 镜像，Ignis 固定使用平台内置的 `ghcr.io/igniscloud/agents/agent-service:latest`。

### 14.3 环境变量

云端只需要一个模型调用密钥：

```bash
export OPENAI_API_KEY="..."
```

`OPENAI_API_KEY` 供 Codex CLI 调 OpenAI 使用。如果容器已经通过其他方式完成 Codex 登录，也可以不使用 `OPENAI_API_KEY`，但第一版推荐直接用环境变量。

### 14.4 启动服务

```bash
cd /home/hy/workplace/ignis/deploy/agent-service
podman compose up -d
```

默认映射：

```text
host 127.0.0.1:3900 -> container 0.0.0.0:3900
```

健康检查：

```bash
curl -sS http://127.0.0.1:3900/healthz
```

预期返回：

```json
{"ok":true}
```

查看日志：

```bash
podman logs -f agent-service
```

停止服务：

```bash
podman compose down
```

### 14.5 source service 如何调用

来源 service 提交任务：

```bash
curl -sS \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "请访问这个小红书链接并按 schema 输出拆解结果：http://xhslink.com/o/5RZBGIk4WhR",
    "callback_url": "http://source-service.internal/api/agent-callback",
    "task_result_json_schema": {
      "type": "object",
      "required": ["summary"],
      "properties": {
        "summary": { "type": "string" }
      }
    }
  }' \
  http://127.0.0.1:3900/v1/tasks
```

返回：

```json
{
  "task_id": "task_..."
}
```

任务完成后，`agent-service` 会调用：

```text
callback_url
```

callback body：

```json
{
  "task_id": "task_...",
  "status": "succeeded",
  "result": {
    "summary": "..."
  }
}
```

### 14.6 网络说明

`compose.yaml` 默认加入外部网络：

```yaml
networks:
  shared:
    external: true
    name: deploy_shared
```

如果云端还没有这个网络，先创建：

```bash
podman network create deploy_shared
```

如果来源 service 和 `agent-service` 在同一个 Podman network 内，`callback_url` 可以写 service DNS 名，例如：

```text
http://source-service:3000/api/agent-callback
```

如果来源 service 在宿主机上，Linux Podman 可使用：

```text
http://host.containers.internal:<port>/api/agent-callback
```

### 14.7 云端安全边界

`agent-service` 会启动：

```text
codex exec --dangerously-bypass-approvals-and-sandbox
```

所以云端运行时必须把安全边界放在容器层：

- 不挂载宿主机敏感目录
- 不挂载 `/run/podman/podman.sock`
- 不挂载 Docker socket
- 只持久化 `/app/data` 和 `/app/work`
- `callback_host_allowlist` 只允许内部服务域名
- 不把 `3900` 直接暴露到公网
- 对外产品部署时不暴露公网入口，只允许同 project 内部 service 访问

第一版 compose 只绑定：

```text
127.0.0.1:3900
```

对外产品部署不要把这个服务放到公网 listener 后面。

## 15. 通过 Ignis CLI 部署

面向对外产品时，推荐走 Ignis CLI 的 `agent` service 类型，而不是让用户手工管理 Podman compose。

凡是产品需求需要 LLM 或 agent 能力，默认优先使用 `agent-service` 这个平台抽象，而不是在业务 `http` service 中直接通过 HTTP 调模型 provider。业务 service 负责整理 prompt、声明 `task_result_json_schema`、接收 callback 或轮询结果；模型凭据、agent runtime、MCP 工具、结果校验和执行隔离交给 `agent-service`。

### 15.1 创建 service

在 Ignis 项目目录执行：

```bash
ignis service new \
  --service agent-service \
  --kind agent \
  --path services/agent-service
```

CLI 会在 `ignis.hcl` 中生成一个 internal-only 的 `kind = "agent"` 服务：

```hcl
{
  name = "agent-service"
  kind = "agent"
  path = "services/agent-service"
}
```

Ignis 固定注入内置镜像、端口、workdir、MCP URL、数据库路径、workspace 路径和 callback host allowlist。当前版本不支持用户自定义 agent 镜像，也不要求用户配置 `agent = { ... }` 或 `env = { ... }`。

OpenCode 版本使用同样的 internal agent 语义，只需要显式选择 runtime：

```bash
ignis service new \
  --service opencode-agent-service \
  --kind agent \
  --runtime opencode \
  --path services/opencode-agent-service
```

对应的 `ignis.hcl` 为：

```hcl
{
  name = "opencode-agent-service"
  kind = "agent"
  agent_runtime = "opencode"
  path = "services/opencode-agent-service"
}
```

`opencode-agent-service` 不需要 `OPENAI_API_KEY` secret。发布时 CLI 会读取 `services/opencode-agent-service/opencode.json`，并把 service 目录下可选的 `skills/` 一起打进 agent bundle。node-agent 启动容器时把 `opencode.json` 只读注入到 `$HOME/.config/opencode/opencode.json`，并把 skills 只读挂载到 `$HOME/.agents/skills`。

### 15.2 配置 secrets

Codex 部署前必须把 secret 写入 IgnisCloud：

```bash
ignis service secrets set \
  --service agent-service \
  openai-api-key \
  "$OPENAI_API_KEY"
```

### 15.3 发布和部署

```bash
ignis service check --service agent-service
ignis service build --service agent-service
ignis service publish --service agent-service
```

`publish` 返回版本号后部署：

```bash
ignis service deploy --service agent-service <version>
```

查看状态和日志：

```bash
ignis service status --service agent-service
ignis service logs --service agent-service --limit 100
```

### 15.4 来源 service 调用地址

`agent-service` 默认不声明公网 exposure，因此不会出现在公网 listener 上。来源 service 必须是同一个 project 内的 service，通过内部 service 路由访问：

```text
POST http://agent-service.svc/v1/tasks
```

请求体包含：

```json
{
  "prompt": "...",
  "callback_url": "http://source-service.internal/api/agent-callback",
  "task_result_json_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": ["message"],
    "properties": {
      "message": {
        "type": "string"
      }
    }
  }
}
```

其中 `callback_url` 可选。`task_result_json_schema` 是这个任务最终结果的 JSON Schema，`agent-service` 会在 `submit_task` 时校验 agent 提交的 `result`。

`agent-service` 立即返回：

```json
{
  "task_id": "task_..."
}
```

如果请求提供 `callback_url`，任务完成后 `agent-service` 调用该地址回传结果。如果没有提供 `callback_url`，来源 service 通过轮询读取结果：

```text
GET http://agent-service.svc/v1/tasks/{task_id}
```

响应：

```json
{
  "task_id": "task_...",
  "status": "queued | running | succeeded | failed",
  "result": null,
  "error": null
}
```

成功后：

```json
{
  "task_id": "task_...",
  "status": "succeeded",
  "result": {
    "message": "..."
  },
  "error": null
}
```

### 15.5 OpenCode fullstack 创建步骤

最小 fullstack 形态由三个 service 组成：

1. `web`：前端，发送消息到后端。
2. `api`：WASI HTTP 后端，创建任务并轮询结果。
3. `agent-service`：internal `agent` service，运行 OpenCode。

创建 OpenCode agent service：

```bash
ignis service new \
  --service agent-service \
  --kind agent \
  --runtime opencode \
  --path services/agent-service
```

`ignis.hcl` 里的 service 形态：

```hcl
{
  name = "agent-service"
  kind = "agent"
  agent_runtime = "opencode"
  path = "services/agent-service"

  resources {
    memory_limit_bytes = 536870912
  }
}
```

把当前机器的 OpenCode 配置复制到 service 目录：

```bash
cp ~/.config/opencode/opencode.json services/agent-service/opencode.json
chmod 600 services/agent-service/opencode.json
```

`opencode.json` 可能包含 provider 凭据，应放在版本控制之外，并避免打印到日志。发布时 CLI 会把该文件放进 OpenCode agent bundle；node-agent 启动容器时会只读挂载到：

```text
/agent-home/.config/opencode/opencode.json
```

内置 OpenCode agent 容器的 entrypoint 会设置 `OPENCODE_CONFIG` 指向这个路径，然后启动 `agent-service --runtime opencode`。

如果需要给 agent 增加长期角色说明，放在 agent service 目录的 `AGENTS.md`：

```text
services/agent-service/
  AGENTS.md
```

发布时 CLI 会把 `AGENTS.md` 一起打进 agent bundle；部署时 node-agent 会只读挂载到：

```text
/app/config/AGENTS.md
```

`agent-service` 启动后会把内置 one-task 系统提示词和这个文件合并，并写入运行工作目录的 `AGENTS.md`。

如果需要自定义 skills，把它们放在 agent service 目录：

```text
services/agent-service/
  skills/
    my-skill/
      SKILL.md
      references/
        ...
```

发布时 CLI 会把 `skills/` 一起打进 agent bundle；部署时 node-agent 会挂载到：

```text
/agent-home/.agents/skills
```


后端创建任务示例：

```json
{
  "prompt": "Respond to the user message below. Return only the final JSON object through submit_task. User message: hello",
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
}
```

部署顺序：

```bash
ignis service check --service agent-service
ignis service build --service agent-service
ignis service publish --service agent-service
ignis service deploy --service agent-service <version>
```

后端和前端按普通 `http` / `frontend` service 发布部署。完整示例见：

```text
examples/opencode-agent-e2e
```

### 15.6 线上 node-agent 注意事项

线上 node-agent 在容器里运行，但 agent 容器由宿主 Podman socket 启动。此时不能把 agent endpoint 记录成 `127.0.0.1:<port>` 给 node-agent 容器访问，因为那会指向 node-agent 容器自身。当前 compose 需要：

```yaml
environment:
  CONTAINER_HOST: unix:///run/podman/podman.sock
  IGNISCLOUD_AGENT_PORT_BIND_HOST: 10.89.2.1
  IGNISCLOUD_AGENT_ENDPOINT_HOST: host.containers.internal
```

并挂载宿主 Podman socket：

```yaml
volumes:
  - /run/podman/podman.sock:/run/podman/podman.sock
```

这样 node-agent 启动的 agent 容器会发布成类似：

```text
10.89.2.1:<host_port> -> 3900/tcp
```

WASI 后端访问 `http://agent-service.svc` 时，node-agent 内部代理会转发到 `http://host.containers.internal:<host_port>`。
