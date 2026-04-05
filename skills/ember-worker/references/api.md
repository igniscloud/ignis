# API 文档

本文描述 `ember` 当前公开可用的 API 面。这里的 API 分成两类：

- Rust crate API
- `ember-cli` 兼容控制面所依赖的 HTTP API

## 1. Rust crate API

### 1.1 `ember-manifest`

主要能力：

- 读取 `worker.toml`
- 校验 manifest
- 渲染 TOML
- 对组件做 Ed25519 签名和验签

主要公开项：

- `MANIFEST_FILE`
- `WorkerManifest`
- `SqliteConfig`
- `ResourceConfig`
- `NetworkConfig`
- `NetworkMode`
- `ComponentSignature`
- `TrustedSigner`
- `LoadedManifest`
- `sign_component_with_seed(...)`
- `verify_component_signature(...)`

核心字段：

- `name`
- `component`
- `base_path`
- `[env]`
- `[secrets]`
- `[sqlite].enabled`
- `[resources].cpu_time_limit_ms`
- `[resources].memory_limit_bytes`
- `[network].mode`
- `[network].allow`

### 1.2 `ember-sdk`

#### `ember_sdk::http`

主要类型：

- `Router`
- `Context`
- `Middleware`
- `Next`

主要方法：

- `Router::new()`
- `use_middleware(...)`
- `route(...)`
- `get(...)`
- `post(...)`
- `put(...)`
- `patch(...)`
- `delete(...)`
- `options(...)`
- `handle(req).await`

`Context` 提供：

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

#### `ember_sdk::sqlite`

主要函数：

- `execute(sql, params)`
- `query(sql, params)`
- `execute_batch(sql)`
- `transaction(statements)`
- `query_typed(sql, params)`

迁移 helper：

- `sqlite::migrations::Migration`
- `sqlite::migrations::apply(migrations)`

### 1.3 `ember-runtime`

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

运行时当前负责：

- 组件装载
- WASI / `wasi:http` 链接
- HTTP 请求分发
- CPU / memory 限制
- `base_path` 重写
- 出站网络控制

### 1.4 `ember-platform-host`

主要类型：

- `SqliteHost`
- `HostBindings`

作用：

- 通过 `HostBindings` 把平台宿主挂到运行时
- 通过 `SqliteHost` 提供第一个 SQLite host import 实现

## 2. `ember-cli` 兼容控制面 HTTP API

CLI 当前要求 Bearer Token 鉴权，并使用这些接口：

### 身份接口

- `GET /v1/whoami`
- `POST /v1/logout`

`GET /v1/whoami` 的响应里，CLI 会读取：

- `data.sub`
- `data.aud`
- `data.display_name`

### 应用与版本接口

- `POST /v1/apps/{app}/versions`
- `POST /v1/apps/{app}/deployments`
- `POST /v1/apps/{app}/rollback`
- `DELETE /v1/apps/{app}/versions/{version}`
- `DELETE /v1/apps/{app}`

`POST /v1/apps/{app}/versions` 使用 `multipart/form-data`，字段包括：

- `manifest`
- `build_metadata`
- `component`
- `signature` 可选

### 查询接口

- `GET /v1/apps`
- `GET /v1/apps/{app}`
- `GET /v1/nodes`
- `GET /v1/apps/{app}/deployments/history?limit={n}`
- `GET /v1/apps/{app}/events?limit={n}`
- `GET /v1/apps/{app}/logs?limit={n}`

### 环境变量与 Secret 接口

- `GET /v1/apps/{app}/env`
- `POST /v1/apps/{app}/env`
- `DELETE /v1/apps/{app}/env/{name}`
- `GET /v1/apps/{app}/secrets`
- `POST /v1/apps/{app}/secrets`
- `DELETE /v1/apps/{app}/secrets/{name}`

### SQLite 接口

- `GET /v1/apps/{app}/sqlite/backup`
- `POST /v1/apps/{app}/sqlite/restore`

`GET /v1/apps/{app}/sqlite/backup` 的 JSON 响应需要包含：

- `data.sqlite_base64`

为了兼容旧响应，CLI 也接受：

- `data.data.sqlite_base64`

## 3. 组件签名约定

`ember publish` 支持这些环境变量：

- `EMBER_SIGNING_KEY_ID`
- `EMBER_SIGNING_KEY_BASE64`

为了兼容旧链路，也接受：

- `WKR_SIGNING_KEY_ID`
- `WKR_SIGNING_KEY_BASE64`
