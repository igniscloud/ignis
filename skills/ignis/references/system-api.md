# System API

Ignis exposes a small set of platform-owned APIs to services at runtime. Use these APIs for platform metadata, same-project service discovery, and platform-managed storage signing.

## Reserved Platform Service

`__ignis.svc` is a reserved internal service name. Application services must not use `__ignis` as a service name.

User HTTP services can call:

```text
GET http://__ignis.svc/v1/services
```

The response lists services in the caller's project:

```json
{
  "data": [
    {
      "project": "demo",
      "service_key": "demo/api",
      "service": "api",
      "kind": "http",
      "service_identity": "svc://demo/api",
      "service_url": "http://api.svc",
      "active_version": "v1",
      "active_node_name": "node-a"
    },
    {
      "project": "demo",
      "service_key": "demo/research-agent",
      "service": "research-agent",
      "kind": "agent",
      "service_identity": "svc://demo/research-agent",
      "service_url": "http://research-agent.svc",
      "metadata_url": "http://research-agent.svc/v1/metadata",
      "runtime": "opencode",
      "memory": "none",
      "description": "Researches external information and returns structured evidence.",
      "active_version": "v1",
      "active_node_name": "node-a"
    }
  ]
}
```

For TaskPlan, filter `kind = "agent"` and use `description`, `runtime`, `memory`, `service_url`, and `metadata_url` to build the coordinator's `available_agents` input.

## Agent Metadata

Every `agent` service must define `agent_description` in `ignis.hcl`:

```hcl
{
  name = "research-agent"
  kind = "agent"
  agent_runtime = "opencode"
  agent_memory = "none"
  agent_description = "Researches external information and returns structured evidence."
  path = "services/research-agent"
}
```

IgnisCloud/node-agent writes `agent_description` into the managed agent-service config. The agent service exposes it through:

```text
GET http://research-agent.svc/v1/metadata
```

Response:

```json
{
  "runtime": "opencode",
  "memory": "none",
  "description": "Researches external information and returns structured evidence."
}
```

## Object Store Presign

Wasm HTTP services should use the guest SDK instead of calling control-plane signing endpoints directly:

```rust
use ignis_sdk::object_store;

let upload = object_store::presign_upload(
    "demo.txt",
    "text/plain",
    12,
    None,
    Some(15 * 60 * 1000),
)?;

let download = object_store::presign_download(&upload.file_id, Some(15 * 60 * 1000))?;
```

The SDK calls the platform host import `ignis:platform/object-store`. The node-agent host import forwards the request to the control plane for the current project. The service and browser never receive COS/S3 credentials.

The control-plane routes behind this flow are platform/internal implementation details:

```text
POST /v1/internal/projects/{project}/files/presign-upload
POST /v1/internal/projects/{project}/files/{file_id}/presign-download
```

Read [`object-store-presign.md`](object-store-presign.md) for the full upload/download flow and example.

## Boundaries

- `__ignis.svc` endpoints are internal project runtime APIs, not public internet APIs.
- `__ignis.svc` is resolved by node-agent and is available only inside a running Ignis service.
- Object-store presign should go through `ignis_sdk::object_store` from Wasm services.
- User-facing APIs should usually be implemented by the project's own HTTP service, which can call these system APIs internally.
