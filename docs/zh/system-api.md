# System API

Ignis 在运行时向 service 暴露一组平台内置 API。它们用于读取平台元数据、查询同 project 内的 service，以及使用平台托管的对象存储签名能力。

## 保留平台 service

`__ignis.svc` 是保留的内部 service 名。用户自己的 service 不能命名为 `__ignis`。

HTTP service 可以调用：

```text
GET http://__ignis.svc/v1/services
```

响应会列出调用方 project 内的全部 service：

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

TaskPlan coordinator 应该从这个接口拿到全部 service，自己筛选 `kind = "agent"`，再使用 `description`、`runtime`、`memory`、`service_url` 和 `metadata_url` 构造 `available_agents`。

## Agent Metadata

每个 `agent` service 必须在 `ignis.hcl` 中定义 `agent_description`：

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

IgnisCloud/node-agent 会把 `agent_description` 写入托管 agent-service 配置。agent service 会通过下面的接口暴露这些元数据：

```text
GET http://research-agent.svc/v1/metadata
```

响应：

```json
{
  "runtime": "opencode",
  "memory": "none",
  "description": "Researches external information and returns structured evidence."
}
```

## Object Store Presign

Wasm HTTP service 应通过 guest SDK 请求对象存储签名，不要直接调用 control-plane 内部接口：

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

SDK 会调用平台 host import `ignis:platform/object-store`。node-agent 的 host import 会把请求转发到当前 project 对应的 control-plane。service 和浏览器都不会拿到 COS/S3 凭证。

这个流程背后的 control-plane route 是平台内部实现细节：

```text
POST /v1/internal/projects/{project}/files/presign-upload
POST /v1/internal/projects/{project}/files/{file_id}/presign-download
```

完整上传/下载流程和示例见 [`object-store-presign.md`](object-store-presign.md)。

## 边界

- `__ignis.svc` 是 project 内运行时 API，不是公网 API。
- `__ignis.svc` 由 node-agent 解析，只能在运行中的 Ignis service 内访问。
- Wasm service 中的对象存储签名应通过 `ignis_sdk::object_store` 使用。
- 用户对外暴露的 API 通常应由 project 自己的 HTTP service 实现，再由它在内部调用这些 system API。
