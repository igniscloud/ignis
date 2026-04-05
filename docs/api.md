# API 文档

本文描述 `ember` 当前公开可用的 API 面。这里的 API 分成两类：

- Rust crate API
- `ember-cli` 兼容控制面所依赖的 HTTP API

`ember` 当前不实现公开控制面，因此这里的 HTTP API 文档描述的是“CLI 期望的接口契约”，而不是本仓库内置服务。

## 1. Rust crate API

### 1.1 `ember-manifest`

`ember-manifest` 负责 `worker.toml` 的读取、校验、渲染和组件签名。

#### 常量

- `MANIFEST_FILE = "worker.toml"`

#### 主要类型

- `WorkerManifest`
- `SqliteConfig`
- `ResourceConfig`
- `NetworkConfig`
- `NetworkMode`
- `ComponentSignature`
- `TrustedSigner`
- `LoadedManifest`

#### 主要能力

- `LoadedManifest::load(path)`
  从目录或显式文件路径读取 manifest
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

#### `worker.toml` 字段

最常见结构：

```toml
name = "hello-worker"
component = "target/wasm32-wasip2/release/hello_worker.wasm"
base_path = "/"

[env]

[secrets]

[sqlite]
enabled = true

[resources]
cpu_time_limit_ms = 5000
memory_limit_bytes = 134217728

[network]
mode = "deny_all"
allow = []
```

字段说明：

- `name`
  worker 名称，只允许字母、数字、`-`、`_`
- `component`
  Wasm 产物的相对路径
- `base_path`
  运行时会先按这里重写请求路径，必须以 `/` 开头
- `[env]`
  注入给 guest 的普通环境变量
- `[secrets]`
  secret 映射。Ember 本身不实现 secret 存储，具体解析由外部平台决定
- `[sqlite].enabled`
  是否开启 SQLite host import
- `[resources].cpu_time_limit_ms`
  CPU 时间上限，运行时通过 epoch interruption 执行限制
- `[resources].memory_limit_bytes`
  内存上限
- `[network].mode`
  出站网络策略，可选 `deny_all`、`allow_list`、`allow_all`
- `[network].allow`
  仅在 `allow_list` 模式下生效，支持 `host`、`host:port`、`.suffix`、`[ipv6]:port`

### 1.2 `ember-sdk`

`ember-sdk` 是 guest 侧 Rust SDK，当前主要分为 `http` 和 `sqlite` 两块。

#### `ember_sdk::http`

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
use ember_sdk::http::{Context, Router, middleware, text_response};
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

#### `ember_sdk::sqlite`

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
use ember_sdk::sqlite::{self, SqliteValue};

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

### 1.3 `ember-runtime`

`ember-runtime` 负责组件装载、WASI / `wasi:http` 链接、请求分发、资源限制和出站网络控制。

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

### 1.4 `ember-platform-host`

`ember-platform-host` 是平台侧宿主扩展层。当前只包含 SQLite 实现。

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

## 2. `ember-cli` 兼容控制面 HTTP API

本节描述 `ember-cli` 期待的平台接口。只要你的控制面实现这些接口，CLI 就可以工作。

### 2.1 认证方式

CLI 当前使用：

- `Authorization: Bearer <token>`

登录不是用户名密码交换，而是“保存一个已签发的 API token”，并用 `GET /v1/whoami` 验证该 token 是否可用。

### 2.2 基础 URL

CLI 会把 `--server` 与固定路径拼接。例如：

```text
https://your-platform.example.com + /v1/apps
```

也就是说，如果你把 API 挂在 `/api` 下，应传入：

```text
https://your-platform.example.com/api
```

### 2.3 身份接口

#### `GET /v1/whoami`

用途：

- 登录时验证 token
- `ember whoami`

CLI 会读取响应中的：

- `data.sub`
- `data.aud`
- `data.display_name`

#### `POST /v1/logout`

用途：

- `ember logout`

CLI 不要求响应体格式，只要求返回成功状态码。

### 2.4 应用与版本接口

#### `POST /v1/apps/{app}/versions`

用途：

- `ember publish`

请求格式：

- `multipart/form-data`

表单字段：

- `manifest`
  JSON，内容为 `worker.toml` 解析后的 manifest
- `build_metadata`
  JSON，包含 builder、manifest_path、component_path、build_mode
- `component`
  Wasm 二进制，`application/wasm`
- `signature`
  可选 JSON，对应 `ComponentSignature`

#### `POST /v1/apps/{app}/deployments`

用途：

- `ember deploy <app> <version>`

请求 JSON：

```json
{ "version": "<version>" }
```

#### `POST /v1/apps/{app}/rollback`

用途：

- `ember rollback <app> <version>`

请求 JSON：

```json
{ "version": "<version>" }
```

#### `DELETE /v1/apps/{app}/versions/{version}`

用途：

- `ember delete-version <app> <version>`

#### `DELETE /v1/apps/{app}`

用途：

- `ember delete-app <app>`

### 2.5 查询接口

#### `GET /v1/apps`

用途：

- `ember apps`

#### `GET /v1/apps/{app}`

用途：

- `ember status <app>`

#### `GET /v1/nodes`

用途：

- `ember nodes`

#### `GET /v1/apps/{app}/deployments/history?limit={n}`

用途：

- `ember deployments <app> --limit <n>`

#### `GET /v1/apps/{app}/events?limit={n}`

用途：

- `ember events <app> --limit <n>`

#### `GET /v1/apps/{app}/logs?limit={n}`

用途：

- `ember logs <app> --limit <n>`

### 2.6 环境变量接口

#### `GET /v1/apps/{app}/env`

用途：

- `ember env list <app>`

#### `POST /v1/apps/{app}/env`

用途：

- `ember env set <app> <name> <value>`

请求 JSON：

```json
{ "name": "<name>", "value": "<value>" }
```

#### `DELETE /v1/apps/{app}/env/{name}`

用途：

- `ember env delete <app> <name>`

### 2.7 Secret 接口

#### `GET /v1/apps/{app}/secrets`

用途：

- `ember secrets list <app>`

#### `POST /v1/apps/{app}/secrets`

用途：

- `ember secrets set <app> <name> <value>`

请求 JSON：

```json
{ "name": "<name>", "value": "<value>" }
```

#### `DELETE /v1/apps/{app}/secrets/{name}`

用途：

- `ember secrets delete <app> <name>`

### 2.8 SQLite 备份与恢复接口

#### `GET /v1/apps/{app}/sqlite/backup`

用途：

- `ember sqlite backup <app> <out>`

CLI 期望响应 JSON 中存在：

- `data.sqlite_base64`

兼容旧响应时，CLI 也会接受：

- `data.data.sqlite_base64`

#### `POST /v1/apps/{app}/sqlite/restore`

用途：

- `ember sqlite restore <app> <input>`

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

`ember publish` 支持对组件签名。

环境变量：

- `EMBER_SIGNING_KEY_ID`
- `EMBER_SIGNING_KEY_BASE64`

为了兼容旧链路，CLI 当前也会接受：

- `WKR_SIGNING_KEY_ID`
- `WKR_SIGNING_KEY_BASE64`

签名算法：

- Ed25519

签名对象：

- 构建产出的 Wasm 组件字节流

## 4. 文档与源码对应关系

如果你需要确认本文是否与实现一致，优先查看：

- `crates/ember-manifest/src/lib.rs`
- `crates/ember-sdk/src/lib.rs`
- `crates/ember-runtime/src/lib.rs`
- `crates/ember-platform-host/src/lib.rs`
- `crates/ember-cli/src/api.rs`
- `crates/ember-cli/src/main.rs`
