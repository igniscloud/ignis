# 接入文档

本文说明如何接入 `ember`。这里的“接入”分成两类：

- 作为 worker 开发者，使用 `ember-cli`、`ember-sdk`、`worker.toml` 开发一个 Wasm HTTP 服务
- 作为平台开发者，把 `ember-manifest`、`ember-runtime`、`ember-platform-host` 组装进你自己的宿主或控制面

`ember` 当前不包含公开的控制面实现或节点编排系统，但已经包含本地开发所需的 CLI、manifest、运行时和第一个平台宿主模块。

## 1. 适用场景

如果你的目标是：

- 写一个运行在 Wasm 里的 HTTP 服务
- 在本地快速调试 `wasi:http` worker
- 在你自己的平台里嵌入 Wasm 运行时
- 复用 `worker.toml`、签名、SQLite host import、路由 SDK

那么 `ember` 已经可以作为基础。

如果你的目标是：

- 直接使用现成的公开控制面
- 直接拿到多节点调度、租户管理、部署编排

那么这些能力不在当前仓库里，需要你在外部平台自行实现。

## 2. 工作区组成

`ember` 当前主要由这些 crate 组成：

- `ember-cli`
  用于初始化项目、本地构建/调试，以及调用兼容的外部控制面 API
- `ember-manifest`
  负责 `worker.toml` 解析、校验、渲染和组件签名
- `ember-sdk`
  提供 guest 侧 Rust SDK，包括 HTTP Router 和 SQLite helper
- `ember-runtime`
  提供基于 Wasmtime 的运行时，用于装载和执行 `wasi:http` 组件
- `ember-platform-host`
  提供第一个平台宿主实现，目前是 SQLite host import

## 3. 作为 worker 开发者接入

### 3.1 前置条件

建议准备：

- Rust toolchain
- `wasm32-wasip2` target

安装 target：

```bash
rustup target add wasm32-wasip2
```

如果你希望优先用 `cargo component build`，还可以自行安装 `cargo-component`。`ember build` 在检测不到它时会自动回退到标准 `cargo build --target wasm32-wasip2`。

安装 CLI：

```bash
cargo install --git https://github.com/pleasewhy/ember ember-cli
```

### 3.2 初始化一个 worker

如果你还没有源码仓库，可以先克隆公开仓库：

```bash
git clone https://github.com/pleasewhy/ember.git
cd ember
ember init hello-worker
cd hello-worker
```

默认会生成：

- `Cargo.toml`
- `src/lib.rs`
- `wit/world.wit`
- `worker.toml`
- `.gitignore`

初始化模板是一个最小 `wasi:http/proxy` worker，不强依赖 `ember-sdk`。这样你可以先跑通最小链路，再决定是否引入 SDK。

### 3.3 本地构建与调试

构建：

```bash
ember build
```

本地运行：

```bash
ember dev --addr 127.0.0.1:3000
```

访问：

```bash
curl http://127.0.0.1:3000/
```

如果刚刚已经构建过，也可以跳过构建步骤：

```bash
ember dev --skip-build --addr 127.0.0.1:3000
```

### 3.4 引入 `ember-sdk`

当你的接口不再是单一路径时，推荐引入 `ember-sdk`：

```toml
[dependencies]
ember-sdk = "<ember-version>"
http-body-util = "0.1.3"
wstd = "0.6"
```

一个最小 Router 写法：

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
        .get("/", |_context: Context| async move {
            text_response(StatusCode::OK, "hello from ember\n")
        })
        .expect("register GET /");

    router
        .get("/users/:id", |context: Context| async move {
            let id = context.param("id").unwrap_or("unknown");
            text_response(StatusCode::OK, format!("user={id}\n"))
        })
        .expect("register GET /users/:id");

    router
}
```

### 3.5 使用 `worker.toml`

`worker.toml` 是 Ember worker 的描述文件。最常见的一份配置如下：

```toml
name = "hello-worker"
component = "target/wasm32-wasip2/release/hello_worker.wasm"
base_path = "/"

[env]
APP_NAME = "hello-worker"

[secrets]
OPENAI_API_KEY = "secret://openai-api-key"

[sqlite]
enabled = true

[resources]
cpu_time_limit_ms = 5000
memory_limit_bytes = 134217728

[network]
mode = "deny_all"
```

这些字段控制：

- worker 名称
- Wasm 构件路径
- 路由基础路径
- 普通环境变量
- secret 映射
- SQLite 能力开关
- 资源限制
- 出站网络策略

### 3.6 发布到兼容控制面

`ember` 仓库不带公开控制面，但 `ember-cli` 当前默认会调用托管的 `embercloud` 控制面：

```bash
ember login
ember publish
ember deploy hello-worker <version>
ember status hello-worker
```

这里的 `ember login` 会打开浏览器，走 embercloud 登录页，并通过本地 localhost 回调保存 CLI token。

如果平台支持这些接口，你还可以使用：

- `ember logs`
- `ember env`
- `ember secrets`
- `ember rollback`
- `ember sqlite backup`
- `ember sqlite restore`

控制面兼容接口约定见 [API 文档](./api.md)。

## 4. 作为平台开发者接入

### 4.1 最小集成边界

如果你要把 Ember 嵌进自己的平台，最小接入通常是：

1. 使用 `ember-manifest` 读取并校验 `worker.toml`
2. 使用 `ember-runtime` 装载组件并转发 HTTP 请求
3. 使用 `ember-platform-host` 提供当前的 SQLite host imports
4. 自己实现控制面、认证、发布、版本管理、部署和多节点能力

### 4.2 加载 manifest

```rust
use ember_manifest::LoadedManifest;

let loaded = LoadedManifest::load("./worker.toml")?;
```

`LoadedManifest` 会：

- 自动解析 `worker.toml`
- 校验配置合法性
- 推导 `project_dir`
- 提供 `component_path()` 解析实际 Wasm 路径

### 4.3 本地 HTTP 宿主

如果你只需要一个本地宿主进程，可以直接复用：

```rust
use std::net::SocketAddr;

use ember_manifest::LoadedManifest;
use ember_runtime::{DevServerConfig, serve};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let manifest = LoadedManifest::load("./worker.toml")?;
    serve(
        manifest,
        DevServerConfig {
            listen_addr: "127.0.0.1:3000".parse::<SocketAddr>()?,
        },
    )
    .await
}
```

### 4.4 自定义装载与预热

如果你希望自己管理生命周期，可以直接使用 `WorkerRuntime`：

```rust
use std::sync::Arc;

use ember_manifest::LoadedManifest;
use ember_runtime::WorkerRuntime;

let manifest = LoadedManifest::load("./worker.toml")?;
let runtime = Arc::new(WorkerRuntime::load(manifest)?);
runtime.warm().await?;
```

这适合：

- 在进程启动时预热组件
- 把 runtime 缓存在你自己的 HTTP server 或 worker manager 中
- 自己管理连接接入、请求分发、实例生命周期

### 4.5 平台宿主扩展点

`ember-platform-host` 当前暴露了：

- `SqliteHost`
- `HostBindings`

`HostBindings` 是运行时与平台宿主之间的桥接点。默认 `WorkerRuntime` 使用 `SqliteHost`，如果你后续要接入更多平台能力，可以沿这个方向扩展新的宿主类型。

当前 SQLite host 的行为是：

- `worker.toml` 中 `[sqlite].enabled = true` 时允许访问数据库
- 数据库默认落在 worker 项目目录下的 `.wkr/sqlite/default.sqlite3`
- 宿主会把 WIT 中定义的 SQLite imports 链接到 Wasmtime 组件里

### 4.6 平台需要自己负责的部分

`ember` 当前不替你实现：

- 租户、用户、权限模型
- API token、OIDC、OAuth2
- 版本仓库和工件存储
- 部署计划和流量切换
- 多节点调度和节点健康管理
- 发布审批、审计和运维后台

这些能力需要你在外部平台中组合实现。

## 5. 推荐阅读顺序

如果你是 worker 开发者：

1. 先看 [CLI 文档](./cli.md)
2. 再看 [API 文档](./api.md) 中的 `ember-sdk` 和 `worker.toml`
3. 最后参考 `examples/`

如果你是平台开发者：

1. 先看 [API 文档](./api.md)
2. 再看 `ember-runtime`、`ember-platform-host` crate
3. 最后决定自己的控制面接口如何兼容 `ember-cli`
