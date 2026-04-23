# `ignis.hcl` Guide

`ignis.hcl` is the project-level manifest used by Ignis. It defines:

- the project name and current public domain
- listeners and public exposures
- the services that belong to the project
- each service's local relative path
- HTTP runtime configuration for `http` services
- static build configuration for `frontend` services
- optional async jobs and cron schedules

The parsed and validated model is owned by `ignis-manifest`. For the exact implementation, use `crates/ignis-manifest/src/project_hcl.rs` as the source of truth.

## 1. Minimal example

```hcl
project = {
  name = "hello-project"
  domain = "prj-1234567890abcdef.transairobot.com"
}

listeners = [
  {
    name = "public"
    protocol = "http"
  }
]

exposes = [
  {
    name = "api"
    listener = "public"
    service = "api"
    path = "/"
  }
]

services = [
  {
    name = "api"
    kind = "http"
    path = "services/api"
    http = {
      component = "target/wasm32-wasip2/release/api.wasm"
      base_path = "/"
    }
  }
]

jobs = [
  {
    name = "process_upload"
    queue = "default"
    target = {
      service = "api"
      binding = "http"
      path = "/jobs/process-upload"
      method = "POST"
    }
    timeout_ms = 120000
    retry = {
      max_attempts = 3
      backoff = "exponential"
      initial_delay_ms = 5000
      max_delay_ms = 60000
    }
    concurrency = {
      max_running = 1
    }
  }
]

schedules = [
  {
    name = "nightly_upload_digest"
    job = "process_upload"
    cron = "0 2 * * *"
    timezone = "UTC"
    enabled = true
    overlap_policy = "forbid"
    misfire_policy = "skip"
    input = {
      source = "schedule"
    }
  }
]
```

## 2. Full example

```hcl
project = {
  name = "pocket-tasks"
  domain = "pockettasks.transairobot.com"
}

listeners = [
  {
    name = "public"
    protocol = "http"
  }
]

exposes = [
  {
    name = "api"
    listener = "public"
    service = "api"
    path = "/api"
  },
  {
    name = "web"
    listener = "public"
    service = "web"
    path = "/"
  }
]

services = [
  {
    name = "api"
    kind = "http"
    path = "services/api"
    bindings = [
      {
        name = "http"
        kind = "http"
      }
    ]
    http = {
      component = "target/wasm32-wasip2/release/api.wasm"
      base_path = "/api"
    }
    env = {
      APP_NAME = "pocket-tasks"
      LOG_LEVEL = "info"
    }
    secrets = {
      OPENAI_API_KEY = "secret://openai-api-key"
    }
    sqlite = {
      enabled = true
    }
    postgres = {
      enabled = true
    }
    resources = {
      memory_limit_bytes = 134217728
    }
  },
  {
    name = "web"
    kind = "frontend"
    path = "services/web"
    frontend = {
      build_command = ["npm", "run", "build"]
      output_dir = "dist"
      spa_fallback = true
    }
  },
  {
    name = "agent-service"
    kind = "agent"
    agent_description = "Handles one structured agent task and returns JSON output."
    path = "services/agent-service"
  }
]
```

## 3. Main sections

### 3.1 `project`

`project.name`

- required
- string
- used as the display name and project creation input

`project.domain`

- optional
- host only, without `https://`
- updated by the CLI when project creation, sync, and domain operations change the active public host

### 3.2 `listeners`

Each listener represents a public ingress surface. Today Ignis supports HTTP listeners.

Common fields:

- `name`
- `protocol`

### 3.3 `exposes`

Each exposure maps a service binding onto a public listener path.

Common fields:

- `name`
- `listener`
- `service`
- `binding`
- `path`

### 3.4 `services`

Each service block defines one deployable unit inside the project.

Shared fields:

- `name`
- `kind`
- `path`
- `bindings`
- `env`
- `secrets`
- `sqlite`
- `postgres`
- `resources`

`http` services use:

- `http.component`
- `http.base_path`

`postgres.enabled = true` asks a compatible IgnisCloud control plane to create
a service-scoped Postgres database and inject it into the worker host. Guest
code should use `ignis_sdk::postgres`; the database URL is consumed by the host
import and is not exposed to guest code.

External MySQL uses the regular `env` / `secrets` mechanism rather than a
`mysql` manifest block. Configure `IGNIS_MYSQL_URL` as a service secret and use
`ignis_sdk::mysql` from guest code. The runtime host keeps a pooled
`sqlx::MySqlPool` keyed by the URL and pool settings:

```hcl
env = {
  IGNIS_MYSQL_MAX_CONNECTIONS = "64"
  IGNIS_MYSQL_MIN_CONNECTIONS = "4"
  IGNIS_MYSQL_ACQUIRE_TIMEOUT_MS = "5000"
  IGNIS_MYSQL_IDLE_TIMEOUT_MS = "30000"
  IGNIS_MYSQL_MAX_LIFETIME_MS = "600000"
}
secrets = {
  IGNIS_MYSQL_URL = "secret://mysql-url"
}
```

`frontend` services use:

- `frontend.build_command`
- `frontend.output_dir`
- `frontend.spa_fallback`

An `agent` service runs the built-in `agent-service` container on the node-agent. The same container image supports Codex and OpenCode; Podman is an implementation detail. The default runtime is Codex. To use OpenCode, set `agent_runtime = "opencode"` and place an `opencode.json` file in the service directory.

When a product requirement needs LLM or agent behavior, prefer an internal `agent` service and the task API over making direct model-provider HTTP requests from an `http` service. This keeps provider credentials, runtime setup, MCP tools, result validation, callback handling, and polling behind the platform-managed agent boundary.

Ignis injects the fixed image, port, workdir, MCP URL, database path, workspace path, and callback host allowlist. Users do not configure those fields.

The built-in image exposes `POST /v1/tasks`, starts one agent runtime process per task, and stores the schema-validated result. Agent containers use Playwright client libraries and connect to the shared node-level Playwright server. If the task includes `callback_url`, the result is posted there; otherwise callers can poll `GET /v1/tasks/:task_id`.

Create an OpenCode agent service with:

```bash
ignis service new \
  --service agent-service \
  --kind agent \
  --runtime opencode \
  --path services/agent-service
```

After running the command, add agent discovery metadata to the generated service declaration when the agent should be selectable by other services:

```hcl
{
  name = "agent-service"
  kind = "agent"
  agent_runtime = "opencode"
  agent_memory = "none"
  agent_description = "Researches external information and returns structured evidence."
  path = "services/agent-service"
}
```

`agent_memory` controls agent-service runtime memory and defaults to `none`.

Supported values:

- `none`: every task invocation starts a fresh runtime session.
- `session`: TaskPlan continuation invocations can reuse a runtime session scoped to the same `(plan_run_id, agent_service_name)`.

`agent_memory` is an agent-service config field, not a task or TaskPlan field. It is not passed through an environment variable. During deploy, IgnisCloud/node-agent writes it into the managed agent-service config file and mounts that file read-only into the container:

```text
/app/config/agent-service.toml
```

`agent_description` is required for every `agent` service. It describes the agent for service discovery, `GET /v1/metadata`, and TaskPlan coordinator prompts. During deploy, IgnisCloud/node-agent writes it into the managed agent-service config file, so `GET http://agent-service.svc/v1/metadata` returns the same description.

For OpenCode, provide the runtime config in the service directory before publishing:

```bash
cp ~/.config/opencode/opencode.json services/agent-service/opencode.json
chmod 600 services/agent-service/opencode.json
```

`opencode.json` may contain provider credentials, so keep it out of version control and avoid printing it in logs. During publish, Ignis stores it in the agent bundle. During deploy, node-agent mounts it read-only at:

```text
/agent-home/.config/opencode/opencode.json
```

For Codex, you may continue using the `openai-api-key` service secret, or place both local Codex auth files in the service directory before publishing:

```bash
cp ~/.codex/auth.json ~/.codex/config.toml services/agent-service/
chmod 600 services/agent-service/auth.json services/agent-service/config.toml
```

When both files are present, Ignis bundles them and node-agent mounts them into:

```text
/agent-home/.codex/
```

Agent standing instructions can live next to the config in `AGENTS.md`:

```text
services/agent-service/
  opencode.json
  AGENTS.md
```

During publish, Ignis stores `AGENTS.md` in the agent bundle. During deploy, node-agent mounts it read-only at:

```text
/app/config/AGENTS.md
```

At startup, `agent-service` appends that file to the built-in one-task system prompt and writes the merged prompt into the runtime workspace as `AGENTS.md`.

Custom agent skills can also live next to the config in the service directory:

```text
services/agent-service/
  opencode.json
  skills/
    my-skill/
      SKILL.md
      references/
        ...
```

During publish, Ignis bundles `skills/` with the agent artifact. During deploy, node-agent mounts it read-only into the container at:

```text
/agent-home/.agents/skills
```


Services in the same project call the internal agent through:

```text
POST http://agent-service.svc/v1/tasks
GET  http://agent-service.svc/v1/tasks/{task_id}
```

Task creation accepts:

```json
{
  "prompt": "...",
  "callback_url": "optional http or https URL",
  "task_result_json_schema": {
    "type": "object"
  }
}
```

`task_result_json_schema` is the schema for the final `result` passed to `submit_task`. If `callback_url` is omitted, poll `GET /v1/tasks/{task_id}` until `status` is `succeeded` or `failed`.

For multi-agent orchestration, user HTTP services can depend on the Ignis `taskplan` crate and use the agent-service TaskPlan mode. In that mode task creation can also include:

```json
{
  "prompt": "...",
  "tool_callback_url": "http://api.svc/internal/taskplan/tools",
  "task_result_json_schema": {
    "type": "object"
  }
}
```

`tool_callback_url` receives `spawn_task_plan` and TaskPlan-mode `submit_task` callbacks from agent-service. The HTTP service owns the actual TaskPlan executor state and should use `taskplan` for validation, dependency readiness, output binding, child plan creation, and parent resume logic.

To discover other services in the same project, a user HTTP service can call the reserved internal platform endpoint:

```text
GET http://__ignis.svc/v1/services
```

The response lists services in the caller's project. The caller can filter `kind = "agent"` when it needs TaskPlan agents:

```json
{
  "data": [
    {
      "service": "api",
      "kind": "http",
      "service_url": "http://api.svc"
    },
    {
      "service": "research-agent",
      "kind": "agent",
      "service_url": "http://research-agent.svc",
      "metadata_url": "http://research-agent.svc/v1/metadata",
      "runtime": "opencode",
      "memory": "none",
      "description": "Researches external information and returns structured evidence."
    }
  ]
}
```

The TaskPlan executor should filter for `kind = "agent"` and use `description` to build `available_agents`. It can also call each `metadata_url` when it needs live runtime metadata. `__ignis.svc` is reserved for platform discovery and should not be used as an application service name. See [`system-api.md`](system-api.md) for built-in platform URLs such as `__ignis.svc`.

See [`taskplan.md`](taskplan.md) for the framework crate, callback payloads, memory boundary, and coordinator-agent guidance.

Current `agent` constraints:

- Custom agent images are not supported yet.
- Agent services are internal-only by default when they have no public exposure.
- Codex requires the `OPENAI_API_KEY` service secret.
- OpenCode requires `opencode.json`; Ignis injects it into `$HOME/.config/opencode/opencode.json` when the container starts.
- `sqlite`, `postgres`, MySQL host imports, and `ignis_login` are not supported for agent services.

### 3.5 `jobs`

`jobs` declares async job types for the project. A job target is an HTTP endpoint on a service in the same project.

Common fields:

- `name`
- `queue`
- `target.service`
- `target.binding`
- `target.path`
- `target.method`
- `timeout_ms`
- `retry.max_attempts`
- `retry.backoff`
- `retry.initial_delay_ms`
- `retry.max_delay_ms`
- `concurrency.max_running`
- `retention.keep_success_days`
- `retention.keep_failed_days`

Current target constraints:

- `target.service` must reference an `http` or `agent` service.
- `target.binding` currently supports the service's default `http` binding.
- `target.path` must be an absolute path.
- job input must be a JSON object.

### 3.6 `schedules`

`schedules` declares cron-based triggers that enqueue jobs.

Common fields:

- `name`
- `job`
- `cron`
- `timezone`
- `enabled`
- `overlap_policy`
- `misfire_policy`
- `input`

`job` must reference a declared job. `cron` uses 5 fields: `minute hour day month weekday`.

Current policy values:

- `overlap_policy`: `allow`, `forbid`, `replace`
- `misfire_policy`: `skip`, `run_once`, `catch_up`

Read [Jobs and Schedules](./jobs-and-schedules.md) for the runtime behavior and current limitations.

## 4. CLI behavior tied to `ignis.hcl`

- `ignis project create` writes the initial file
- `ignis project sync --mode apply` may write back `project.domain`
- `ignis project sync --mode apply` persists `jobs` and `schedules` to project automation config
- `ignis service new` updates `services`
- service lifecycle commands read the current project and service definitions from this file

## 5. Related documents

- [Quickstart](./quickstart.md)
- [CLI Guide](./cli.md)
- [API Reference](./api.md)
- [Ignis Service Link](./ignis-service-link.md)
- [Jobs and Schedules](./jobs-and-schedules.md)
