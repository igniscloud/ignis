# OpenCode Agent E2E Example

This example wires one frontend, one HTTP backend, and one internal OpenCode
`agent-service` together in a single Ignis project.

Flow:

1. The browser sends a message to `POST /api/tasks`.
2. The `api` service creates a task by calling `http://agent-service.svc/v1/tasks`.
3. The `agent-service` starts OpenCode, exposes the task through MCP, and stores
   the schema-validated result.
4. The browser polls `GET /api/tasks/:task_id`.
5. The `api` service proxies status from `GET http://agent-service.svc/v1/tasks/:task_id`.

## OpenCode config

Provide the OpenCode runtime config at `services/agent-service/opencode.json`.
It may contain provider keys, so keep the real file out of version control. For
local testing on this machine:

```bash
cp ~/.config/opencode/opencode.json services/agent-service/opencode.json
```

The Ignis OpenCode agent bundle stores the file as the service artifact.
During deployment, node-agent injects it into the container at:

```text
/agent-home/.config/opencode/opencode.json
```

The container entrypoint sets `OPENCODE_CONFIG` to that path before running
`agent-service`.

## Custom skills

Optional custom skills can be placed under the agent service directory:

```text
services/agent-service/
  skills/
    my-skill/
      SKILL.md
      references/
        ...
```

`ignis service publish --service agent-service` bundles `skills/` with
`opencode.json`. During deployment, node-agent mounts the skills read-only at:

```text
/agent-home/.agents/skills
```


## Task API

The frontend never calls `agent-service` directly. It calls the `api` service:

```text
POST /api/tasks
GET  /api/tasks/:task_id
```

The `api` service creates the internal agent task with:

```json
{
  "prompt": "Respond to the user message below. Return only the final JSON object through submit_task. User message: <message>",
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

Because this request does not include `callback_url`, the `api` service polls:

```text
GET http://agent-service.svc/v1/tasks/:task_id
```

The stored successful result has this shape:

```json
{
  "message": "..."
}
```

## Validate

```bash
cargo check --manifest-path services/api/Cargo.toml --target wasm32-wasip2
../../target/debug/ignis service check --service agent-service
```
