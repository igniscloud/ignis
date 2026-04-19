# TaskPlan

`taskplan` is an Ignis framework crate for building multi-agent orchestration inside a user-owned HTTP service.

Use it when an application needs a coordinator agent to split work into child plans, dispatch tasks to multiple internal `agent` services, wait for child results, and then resume the coordinator before producing the final result.

## Boundary

`taskplan` lives in Ignis because user service code needs to depend on it.

```text
ignis/crates/taskplan
```

IgnisCloud owns the hosted `agent-service` runtime, container images, deployment, metadata, and service discovery. Ignis owns the framework crate that user HTTP services can use to build a TaskPlan executor.

Recommended split:

```text
User HTTP service
  depends on taskplan
  stores plan/task state
  implements orchestration callbacks
  calls internal agent-service tasks

agent-service
  runs Codex/OpenCode
  exposes MCP tools to the runtime
  forwards spawn_task_plan and submit_task to tool_callback_url in TaskPlan mode

IgnisCloud
  runs and discovers agent services
  stores agent metadata such as description, runtime, and memory
```

The TaskPlan executor is not automatically built into every HTTP service. Only services that need multi-agent orchestration should depend on the crate and implement the storage/invocation adapters they need.

## Dependency

For generated or user-owned HTTP services, depend on the crate from the Ignis repository:

```toml
taskplan = { git = "https://github.com/igniscloud/ignis.git", package = "taskplan", tag = "v0.1.3" }
```

When developing against a local Ignis checkout, use an explicit path dependency instead:

```toml
taskplan = { path = "/path/to/ignis/crates/taskplan" }
```

## Core Model

The first version provides reusable model and validation primitives:

- `TaskPlan`
- `TaskSpec`
- `TaskDependency`
- `OutputBinding`
- `TaskState`
- `ChildPlanLink`
- `validate_plan`
- `ready_task_ids`
- `apply_output_bindings`
- `validate_task_output`

Task states include:

```text
queued
running
waiting_child_plan
succeeded
failed
cancelled
```

`waiting_child_plan` means the current task called `spawn_task_plan`; the parent task is paused until the child plan completes.

## Dynamic Child Plans

Dynamic modification should use append-only child plans instead of arbitrary mutation of an existing plan:

```text
root_plan
  root_task -> main-agent
    spawn child_plan_1
      research -> research-agent
      implement -> coder-agent
      review -> reviewer-agent
    child_plan_1 completes
  root_task resumes with child_plan_1 result
  root_task submits final result through submit_task
```

This keeps history auditable and avoids letting an agent rewrite previous plan state.

## Agent-Service TaskPlan Mode

`agent-service` still supports the simple task API:

```text
POST http://agent-service.svc/v1/tasks
GET  http://agent-service.svc/v1/tasks/{task_id}
```

For TaskPlan mode, the user HTTP service creates an agent task with:

```json
{
  "prompt": "...",
  "tool_callback_url": "http://api.svc/internal/taskplan/tools",
  "task_result_json_schema": {
    "type": "object"
  }
}
```

When `tool_callback_url` is present, agent-service forwards tool results to that URL:

```json
{
  "tool": "spawn_task_plan",
  "task_id": "root",
  "task_plan": {}
}
```

```json
{
  "tool": "submit_task",
  "task_id": "root",
  "status": "succeeded",
  "result": {}
}
```

The HTTP service handles these callbacks by using `taskplan` to update plan state, create child plans, bind child outputs into parent input, and resume the parent task when ready.

## Discovering Agents

The TaskPlan executor should build `available_agents` before invoking the coordinator. Do not ask the coordinator agent to scan the network.

Inside a user HTTP service, call the reserved internal platform endpoint:

```text
GET http://__ignis.svc/v1/services
```

This returns services in the same project. The executor should filter `kind = "agent"`:

```json
{
  "data": [
    {
      "project": "demo",
      "service": "api",
      "kind": "http",
      "service_identity": "svc://demo/api",
      "service_url": "http://api.svc",
      "active_version": "v1"
    },
    {
      "project": "demo",
      "service": "research-agent",
      "kind": "agent",
      "service_identity": "svc://demo/research-agent",
      "service_url": "http://research-agent.svc",
      "metadata_url": "http://research-agent.svc/v1/metadata",
      "runtime": "opencode",
      "memory": "none",
      "description": "Researches external information and returns structured evidence.",
      "active_version": "v1"
    }
  ]
}
```

`description` comes from the agent service's required `agent_description` field in `ignis.hcl`. When the executor needs live runtime metadata, it can call each agent's metadata endpoint:

```text
GET http://research-agent.svc/v1/metadata
```

The metadata endpoint returns the same `description`. The executor should merge that information into the coordinator input:

```json
{
  "available_agents": [
    {
      "name": "research-agent",
      "description": "Researches external information and returns structured evidence.",
      "runtime": "opencode",
      "memory": "none"
    }
  ]
}
```

`__ignis.svc` is reserved for platform discovery and should not be used as an application service name.

## Agent Memory

Agent memory is an `agent-service` config, not a TaskPlan or task field.

Supported values:

```text
none
session
```

Expected semantics:

- `none`: every invocation starts a fresh runtime session.
- `session`: a runtime session is reused for the same `(plan_run_id, agent_service_name)`.

Do not use a global session across different plans, users, or services.

## Coordinator Prompt Guidance

A coordinator agent should be instructed to:

- inspect the available agent services and their descriptions;
- call `spawn_task_plan` when specialist work is needed;
- stop after a successful `spawn_task_plan` call and wait for the system to resume it;
- call `submit_task` only when the final result is complete;
- never put a child plan request inside `submit_task`.

## Current Limits

- `taskplan` currently provides framework primitives, not a complete hosted service.
- The user HTTP service chooses its own persistence.
- Arbitrary in-place plan mutation is not supported; use child plans.
- Human approval nodes and visual workflow editing are out of scope for the first version.
