# API 文档

本文描述 `ignis` 当前公开可用的 API 面。这里的 API 分成两类：

- Rust crate API
- `ignis-cli` 兼容控制面所依赖的 HTTP API

`ignis` 当前不实现公开控制面，因此这里的 HTTP API 文档描述的是“CLI 期望的接口契约”，而不是本仓库内置服务。

## 1. Rust crate API

### 1.1 `ignis-manifest`

`ignis-manifest` 负责 `ignis.hcl`、派生 worker manifest 和组件签名。

#### 常量

- `MANIFEST_FILE = "worker.toml"`
- `PROJECT_MANIFEST_FILE = "ignis.hcl"`

说明：

- `PROJECT_MANIFEST_FILE` 是当前主配置文件
- `MANIFEST_FILE` 只用于内部派生的单 service worker manifest 兼容模型

#### 主要类型

- `ProjectSpec`
- `ListenerSpec`
- `ExposeSpec`
- `ServiceSpec`
- `BindingSpec`
- `CompiledProjectPlan`
- `CompiledServicePlan`
- `CompiledBindingPlan`
- `CompiledExposurePlan`
- `ServiceActivationPlan`
- `ProjectManifest`
- `ProjectConfig`
- `ServiceManifest`
- `ServiceKind`
- `HttpServiceConfig`
- `FrontendServiceConfig`
- `WorkerManifest`
- `SqliteConfig`
- `ResourceConfig`
- `ComponentSignature`
- `TrustedSigner`
- `LoadedProjectManifest`
- `LoadedManifest`

#### 主要能力

- `LoadedProjectManifest::load(path)`
  从目录或显式文件路径读取 `ignis.hcl`
- `LoadedProjectManifest::compiled_plan()`
  返回当前 `ignis.hcl` 编译后的内部项目计划
- `LoadedProjectManifest::find_service(name)`
  查找 project 中的 service
- `LoadedProjectManifest::service_dir(service)`
  返回 service 的本地目录
- `LoadedProjectManifest::http_service_manifest(name)`
  把 `http` service 派生为运行时使用的 `WorkerManifest`
- `LoadedManifest::load(path)`
  从目录或显式文件路径读取单个 worker manifest
- `LoadedManifest::component_path()`
  返回构件的实际路径
- `WorkerManifest::validate()`
  校验 manifest 结构和值
- `WorkerManifest::render()`
  渲染为 TOML
- `sign_component_with_seed(component, key_id, private_seed_base64)`
  对组件进行 Ed25519 签名
- `verify_component_signature(component, signature, trusted_signers)`
  验证组件签名

#### `ignis.hcl` 字段

`ignis.hcl` 的完整字段说明、默认值、校验规则和示例配置见 [ignis.hcl 文档](./ignis-hcl.md)。

### 1.2 `ignis-sdk`

`ignis-sdk` 是 guest 侧 Rust SDK，当前主要分为 `http` 和 `sqlite` 两块。

完整自动生成参考见 [ignis-sdk Markdown 文档](./ignis-sdk/index.md)。

#### `ignis_sdk::http`

主要类型：

- `Router`
- `Context`
- `Middleware`
- `Next`

主要方法：

- `Router::new()`
- `Router::use_middleware(...)`
- `Router::route(...)`
- `Router::get(...)`
- `Router::post(...)`
- `Router::put(...)`
- `Router::patch(...)`
- `Router::delete(...)`
- `Router::options(...)`
- `Router::handle(req).await`

`Context` 当前提供：

- `method()`
- `path()`
- `request()`
- `request_mut()`
- `into_request()`
- `param(name)`
- `params()`
- `request_id()`

内置中间件：

- `middleware::request_id()`
- `middleware::logger()`
- `middleware::cors()`

响应 helper：

- `text_response(status, body)`
- `empty_response(status)`

示例：

```rust
use ignis_sdk::http::{Context, Router, middleware, text_response};
use wstd::http::{Body, Request, Response, Result, StatusCode};

#[wstd::http_server]
async fn main(req: Request<Body>) -> Result<Response<Body>> {
    let router = build_router();
    Ok(router.handle(req).await)
}

fn build_router() -> Router {
    let mut router = Router::new();
    router.use_middleware(middleware::request_id());
    router.use_middleware(middleware::logger());

    router
        .get("/users/:id", |context: Context| async move {
            let id = context.param("id").unwrap_or("unknown");
            text_response(StatusCode::OK, format!("user={id}\n"))
        })
        .expect("register route");

    router
}
```

#### `ignis_sdk::sqlite`

主要类型：

- `QueryResult`
- `Row`
- `SqliteValue`
- `Statement`
- `TypedQueryResult`
- `TypedRow`

主要函数：

- `execute(sql, params)`
- `query(sql, params)`
- `execute_batch(sql)`
- `transaction(statements)`
- `query_typed(sql, params)`

迁移 helper：

- `sqlite::migrations::Migration`
- `sqlite::migrations::apply(migrations)`

示例：

```rust
use ignis_sdk::sqlite::{self, SqliteValue};

fn ensure_schema() -> Result<(), String> {
    sqlite::migrations::apply(&[
        sqlite::migrations::Migration {
            id: "001_create_counters",
            sql: "create table if not exists counters (name text primary key, value integer not null);",
        },
    ])?;
    Ok(())
}

fn read_counter() -> Result<i64, String> {
    let result = sqlite::query_typed(
        "select value from counters where name = ?",
        &["hits"],
    )?;
    let row = result.rows.first().ok_or_else(|| "row missing".to_owned())?;
    match row.values.first() {
        Some(SqliteValue::Integer(value)) => Ok(*value),
        other => Err(format!("unexpected value: {other:?}")),
    }
}
```

### 1.3 `ignis-runtime`

`ignis-runtime` 负责组件装载、WASI / `wasi:http` 链接、请求分发、资源限制和出站网络控制。

主要类型：

- `DevServerConfig`
- `WorkerRuntimeOptions`
- `WorkerRuntime<H = SqliteHost>`

主要函数和方法：

- `serve(manifest, config).await`
- `WorkerRuntime::load(manifest)`
- `WorkerRuntime::load_with_options(manifest, options)`
- `WorkerRuntime::warm().await`
- `WorkerRuntime::manifest()`

行为要点：

- 组件装载基于 Wasmtime component model
- 本地 `serve` 会启动一个 HTTP/1 server
- CPU 限制通过 epoch interruption 生效
- 内存限制通过 store limits 生效
- 出站 HTTP 请求受 `network` 策略控制
- `base_path` 会在请求进入 guest 前被重写

### 1.4 `ignis-platform-host`

`ignis-platform-host` 是平台侧宿主扩展层。当前只包含 SQLite 实现。

主要类型：

- `SqliteHost`
- `HostBindings`

`HostBindings` 负责：

- 从 `LoadedManifest` 构造平台宿主状态
- 把平台宿主 imports 挂到 Wasmtime linker 上

`SqliteHost` 负责：

- 打开 worker 对应的 SQLite 数据库
- 实现 WIT 中约定的 SQLite host functions
- 把 SQLite 功能按 manifest 配置暴露给 guest

## 2. `ignis-cli` 兼容控制面 HTTP API

本节描述 `ignis-cli` 期待的平台接口。只要你的控制面实现这些接口，CLI 就可以工作。

### 2.1 认证方式

CLI 当前使用：

- `Authorization: Bearer <token>`

CLI 默认通过浏览器登录拿到一个可持久化的 CLI token。登录时，CLI 会启动临时 localhost 回调、打开 igniscloud 登录页，并在授权完成后保存返回的 token；如果用户显式传入 `--token`，也仍然会直接把它当作 Bearer token 使用。

### 2.2 基础 URL

CLI 当前固定访问：

```text
https://igniscloud.dev/api
```

例如 project 列表接口是：

```text
https://igniscloud.dev/api/v1/projects
```

### 2.3 身份接口

#### `GET /v1/whoami`

用途：

- 校验传入的 token
- `ignis whoami`

CLI 会读取响应中的：

- `data.sub`
- `data.aud`
- `data.display_name`

### 2.4 Project 接口

说明：

- `POST /v1/projects` 的成功响应应返回远端唯一标识 `data.project_id`，或兼容地返回 `data.id`
- 当前 CLI 会把这个值保存到 `.ignis/project.json`
- 下文 service 相关接口中的路径参数虽然仍记作 `{project}`，但当前 CLI 实际上传递的是 `.ignis/project.json` 中保存的 `project_id`，而不是 `ignis.hcl` 里的 `project.name`

#### `POST /v1/projects`

用途：

- `ignis project create <name>`

请求 JSON：

```json
{ "project_name": "<project>" }
```

#### `GET /v1/projects`

用途：

- `ignis project list`

#### `GET /v1/projects/{project}`

用途：

- `ignis project status <project>`

#### `DELETE /v1/projects/{project}`

用途：

- `ignis project delete <project>`

### 2.5 Project Token 接口

#### `POST /v1/projects/{project}/tokens`

用途：

- `ignis project token create <project>`

请求 JSON：

```json
{ "issued_for": "<optional-label>" }
```

#### `DELETE /v1/projects/{project}/tokens/{token_id}`

用途：

- `ignis project token revoke <project> <token_id>`

### 2.6 Service 与版本接口

#### `POST /v1/projects/{project}/services`

用途：

- `ignis service new --service <name> --kind <kind> --path <path>`

说明：

- 当前 CLI 在 `{project}` 位置传的是远端 `project_id`

请求 JSON：

- 内容为 `ignis.hcl` 编译后的单个 `ServiceManifest`

#### `GET /v1/projects/{project}/services/{service}`

用途：

- `ignis service status --service <service>`

说明：

- 当前 CLI 在 `{project}` 位置传的是远端 `project_id`

#### `POST /v1/projects/{project}/services/{service}/versions`

用途：

- `ignis service publish --service <service>`

说明：

- 当前 CLI 在 `{project}` 位置传的是远端 `project_id`

请求格式：

- `multipart/form-data`

`http` service 表单字段：

- `service_manifest`
  JSON，内容为单个 service 的配置快照
- `build_metadata`
  JSON，包含 builder、project_manifest_path、service_path、build_mode
- `component_sha256`
  组件内容摘要
- `component`
  Wasm 二进制，`application/wasm`
- `signature`
  可选 JSON，对应 `ComponentSignature`

`frontend` service 表单字段：

- `service_manifest`
  JSON，内容为单个 service 的配置快照
- `build_metadata`
  JSON，包含 builder、project_manifest_path、service_path、build_mode
- `site_bundle`
  `tar.gz` 格式的静态站点产物

#### `POST /v1/projects/{project}/services/{service}/deployments`

用途：

- `ignis service deploy --service <service> <version>`

说明：

- 当前 CLI 在 `{project}` 位置传的是远端 `project_id`

请求 JSON：

```json
{ "version": "<version>" }
```

#### `POST /v1/projects/{project}/services/{service}/rollback`

用途：

- `ignis service rollback --service <service> <version>`

说明：

- 当前 CLI 在 `{project}` 位置传的是远端 `project_id`

请求 JSON：

```json
{ "version": "<version>" }
```

#### `DELETE /v1/projects/{project}/services/{service}`

用途：

- `ignis service delete --service <service>`

说明：

- 当前 CLI 在 `{project}` 位置传的是远端 `project_id`

说明：

- 如果 service 仍有 active deployment，请求会失败
- 删除会级联清理该 service 的 env / secrets / versions / deployments / logs

#### `DELETE /v1/projects/{project}/services/{service}/versions/{version}`

用途：

- `ignis service delete-version --service <service> <version>`

说明：

- 当前 CLI 在 `{project}` 位置传的是远端 `project_id`

### 2.7 查询接口

#### `GET /v1/projects/{project}/services/{service}/deployments/history?limit={n}`

用途：

- `ignis service deployments --service <service> --limit <n>`

说明：

- 当前 CLI 在 `{project}` 位置传的是远端 `project_id`

#### `GET /v1/projects/{project}/services/{service}/events?limit={n}`

用途：

- `ignis service events --service <service> --limit <n>`

说明：

- 当前 CLI 在 `{project}` 位置传的是远端 `project_id`

#### `GET /v1/projects/{project}/services/{service}/logs?limit={n}`

用途：

- `ignis service logs --service <service> --limit <n>`

说明：

- 当前 CLI 在 `{project}` 位置传的是远端 `project_id`

### 2.8 环境变量接口

#### `GET /v1/projects/{project}/services/{service}/env`

用途：

- `ignis service env list --service <service>`

#### `POST /v1/projects/{project}/services/{service}/env`

用途：

- `ignis service env set --service <service> <name> <value>`

请求 JSON：

```json
{ "name": "<name>", "value": "<value>" }
```

#### `DELETE /v1/projects/{project}/services/{service}/env/{name}`

用途：

- `ignis service env delete --service <service> <name>`

### 2.9 Secret 接口

#### `GET /v1/projects/{project}/services/{service}/secrets`

用途：

- `ignis service secrets list --service <service>`

#### `POST /v1/projects/{project}/services/{service}/secrets`

用途：

- `ignis service secrets set --service <service> <name> <value>`

请求 JSON：

```json
{ "name": "<name>", "value": "<value>" }
```

#### `DELETE /v1/projects/{project}/services/{service}/secrets/{name}`

用途：

- `ignis service secrets delete --service <service> <name>`

### 2.10 SQLite 备份与恢复接口

#### `GET /v1/projects/{project}/services/{service}/sqlite/backup`

用途：

- `ignis service sqlite backup --service <service> <out>`

CLI 期望响应 JSON 中存在：

- `data.sqlite_base64`

兼容旧响应时，CLI 也会接受：

- `data.data.sqlite_base64`

#### `POST /v1/projects/{project}/services/{service}/sqlite/restore`

用途：

- `ignis service sqlite restore --service <service> <input>`

请求 JSON：

```json
{ "sqlite_base64": "<base64-encoded-sqlite-file>" }
```

### 2.9 响应处理约定

CLI 当前对响应的处理相对宽松：

- 2xx 状态码视为成功
- 如果响应体为空，CLI 会打印一个只带状态码的最小 JSON
- 如果响应体是 JSON，CLI 会原样 pretty-print
- 如果响应体不是 JSON，CLI 会把它包成 `{ "raw": "..." }`

这意味着你可以自行设计大部分响应结构，只要保持状态码和关键字段兼容。

## 3. 组件签名约定

`ignis service publish --service <http-service>` 支持对组件签名。

环境变量：

- `IGNIS_SIGNING_KEY_ID`
- `IGNIS_SIGNING_KEY_BASE64`

签名算法：

- Ed25519

签名对象：

- 构建产出的 Wasm 组件字节流

## 4. 文档与源码对应关系

如果你需要确认本文是否与实现一致，优先查看：

- `crates/ignis-manifest/src/lib.rs`
- `crates/ignis-sdk/src/lib.rs`
- `crates/ignis-runtime/src/lib.rs`
- `crates/ignis-platform-host/src/lib.rs`
- `crates/ignis-cli/src/api.rs`
- `crates/ignis-cli/src/main.rs`
