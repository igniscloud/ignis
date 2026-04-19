# 费马大定理高中生解法 Multi-Agent Workflow

This example demonstrates a TaskPlan-style multi-agent workflow with OpenCode
agent services. The user-facing app asks for a high-school-readable guide to
Fermat's Last Theorem.

Important boundary: this example does not claim to produce a complete
elementary proof of Fermat's Last Theorem. Wiles's proof uses modern number
theory. The workflow produces an honest proof guide that explains the
contradiction chain and clearly labels Ribet's theorem and Wiles's theorem as
black-box theorems.

## Services

The project contains:

- `web`: static frontend for launching and watching the workflow.
- `api`: HTTP service that owns TaskPlan state in SQLite.
- `coordinator-agent`: plans child work, waits for child results, and writes the final guide.
- `elementary-agent`: explains the elementary number theory foundation.
- `bridge-agent`: explains the Frey curve and Ribet theorem bridge.
- `modularity-agent`: explains modularity and Wiles's theorem.
- `teacher-agent`: rewrites specialist outputs for high-school readers.
- `rigor-agent`: checks for mathematical overclaiming and missing black-box labels.

## Flow

1. The browser sends a request to `POST /api/workflows`.
2. The `api` service calls `GET http://__ignis.svc/v1/services` and filters
   `kind = "agent"` to build the available agent list.
3. The `api` service creates a coordinator task at
   `http://coordinator-agent.svc/v1/tasks` with `tool_callback_url`.
4. The coordinator calls `spawn_task_plan`.
5. The `api` service validates and stores the child TaskPlan, dispatches ready
   child tasks to specialist agents, and receives each `submit_task` callback.
6. After child tasks finish, the `api` service starts a coordinator continuation
   task with the child outputs.
7. The coordinator submits the final JSON guide.
8. The browser polls `GET /api/workflows/:run_id` until the workflow succeeds or fails.

## OpenCode config

Each OpenCode agent service needs an `opencode.json` before publishing. For
local testing on this machine:

```bash
for dir in services/*-agent; do
  cp ~/.config/opencode/opencode.json "$dir/opencode.json"
  chmod 600 "$dir/opencode.json"
done
```

The real `opencode.json` files can contain provider credentials. Keep them out
of version control. Each agent directory includes `opencode.json.example`.

During deployment, node-agent injects the config into the agent container at:

```text
/agent-home/.config/opencode/opencode.json
```

## TaskPlan callback

The coordinator and child agents are created with:

```json
{
  "tool_callback_url": "http://api.svc/internal/taskplan/tools",
  "task_result_json_schema": {}
}
```

When the coordinator calls `spawn_task_plan`, agent-service forwards:

```json
{
  "tool": "spawn_task_plan",
  "task_id": "<coordinator-agent-task-id>",
  "task_plan": {
    "id": "fermats-guide-plan",
    "root_task_id": "rigor-review",
    "tasks": []
  }
}
```

When a child or coordinator calls `submit_task`, agent-service forwards:

```json
{
  "tool": "submit_task",
  "task_id": "<agent-task-id>",
  "status": "succeeded",
  "result": {}
}
```

The `api` service persists all state in its SQLite database and dispatches
ready child tasks with the `taskplan` crate.

## API

The frontend calls:

```text
POST /api/workflows
GET  /api/workflows/:run_id
```

Create request:

```json
{
  "question": "请用高中生能看懂的方式解释费马大定理为什么成立。"
}
```

Successful final result shape:

```json
{
  "title": "费马大定理：高中生可读证明导览",
  "important_boundary": "完整证明依赖现代数论；本文解释证明主线并标出黑箱定理。",
  "overview": "...",
  "sections": [
    { "heading": "定理说了什么", "body": "..." }
  ],
  "black_box_theorems": [
    { "name": "Ribet 定理", "plain_language": "..." },
    { "name": "Wiles 的 modularity 定理", "plain_language": "..." }
  ],
  "rigor_notes": ["..."]
}
```

## Validate

```bash
cargo check --manifest-path services/api/Cargo.toml --target wasm32-wasip2
../../target/debug/ignis service check --service api
../../target/debug/ignis service check --service coordinator-agent
../../target/debug/ignis service check --service elementary-agent
../../target/debug/ignis service check --service bridge-agent
../../target/debug/ignis service check --service modularity-agent
../../target/debug/ignis service check --service teacher-agent
../../target/debug/ignis service check --service rigor-agent
```
