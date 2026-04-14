# 接入文档

本文说明如何接入 `ignis`。这里的“接入”分成两类：

- 作为 service 开发者，使用 `ignis-cli`、`ignis-sdk`、`ignis.hcl` 开发一个 project 下的 `http` 或 `frontend` service
- 作为平台开发者，把 `ignis-manifest`、`ignis-runtime`、`ignis-platform-host` 组装进你自己的宿主或控制面

`ignis` 当前不包含公开的控制面实现或节点编排系统，但已经包含 project/service 构建、发布和部署所需的 CLI、manifest、运行时和第一个平台宿主模块。

## 1. 适用场景

如果你的目标是：

- 写一个运行在 Wasm 里的 HTTP service
- 在本地快速调试 `wasi:http` worker
- 在你自己的平台里嵌入 Wasm 运行时
- 复用 `ignis.hcl`、签名、SQLite host import、路由 SDK

那么 `ignis` 已经可以作为基础。

如果你的目标是：

- 直接使用现成的公开控制面
- 直接拿到多节点调度、租户管理、部署编排

那么这些能力不在当前仓库里，需要你在外部平台自行实现。

## 2. 工作区组成

`ignis` 当前主要由这些 crate 组成：

- `ignis-cli`
  用于创建 project、创建 service、本地构建/调试，以及调用兼容的外部控制面 API
- `ignis-manifest`
  负责 `ignis.hcl` / 派生 worker manifest 的解析、校验、渲染和组件签名
- `ignis-sdk`
  提供 guest 侧 Rust SDK，包括 HTTP Router 和 SQLite helper
- `ignis-runtime`
  提供基于 Wasmtime 的运行时，用于装载和执行 `wasi:http` 组件
- `ignis-platform-host`
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

`ignis` 当前统一使用标准 `cargo build --target wasm32-wasip2` 构建 `http` service，不再依赖 `cargo-component`。

安装 CLI：

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://igniscloud.dev/i.sh | sh
```

### 3.2 初始化一个 project 和 service

如果你还没有源码仓库，可以先克隆公开仓库：

```bash
git clone https://github.com/igniscloud/ignis.git
cd ignis
ignis login
ignis project create hello-project
cd hello-project
ignis service new --service api --kind http --path services/api
ignis project sync --mode plan
ignis project sync --mode apply
```

这里的远端绑定规则是：

- `ignis project create hello-project` 会创建远端 project，并把返回的 `project_id` 写入 `hello-project/.ignis/project.json`
- `ignis project create hello-project` 还会把当前线上访问域名写入 `hello-project/ignis.hcl` 的 `project.domain`
- 后续所有 `ignis service ...` 的远端调用都会使用这个 `project_id`
- 如果你拿到的是一个只包含 `ignis.hcl` 的现有仓库副本，没有 `.ignis/project.json`，先在 project 根目录执行 `ignis project sync --mode apply`
- 如果本地 `project.domain` 缺失，`ignis project sync --mode apply` 会自动回填
- 如果本地 `project.domain` 和线上当前域名不一致，`ignis project sync` 会直接报错，要求先修正本地配置

默认会生成：

- `ignis.hcl`
- `services/api/Cargo.toml`
- `services/api/src/lib.rs`
- `services/api/wit/world.wit`
- `services/api/.gitignore`

`service new --kind http` 生成最小 `wasi:http/proxy` service 模板。`service new --kind frontend` 会生成静态前端模板。

### 3.3 构建、发布与部署

构建 `http` service：

```bash
ignis service build --service api
```

构建完成后，直接走发布和部署：

```bash
ignis project sync --mode plan
ignis project sync --mode apply
ignis service check --service api
ignis service publish --service api
ignis service deploy --service api <version>
```

如果 `.ignis/project.json` 已经存在，但 `publish` 或 `deploy` 仍然返回 `404 project <id> not found`，应优先排查 control-plane 是否正确接受了 CLI 传入的 `project_id`；这通常不是因为本地少做了一步初始化。

当前 CLI 以发布部署为主，不再把本地 `dev` 作为标准工作流。测试环境部署能力后续会在 deploy 链路上扩展。

发布前建议先执行：

```bash
ignis service check --service api
```

如果某个 `http` service 需要接入 `IgnisCloud ID` OAuth，推荐在 `ignis.hcl` 里声明：

```hcl
exposes = [
  {
    name = "api"
    listener = "public"
    service = "api"
    path = "/api"
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
    ignis_login = {
      display_name = "hello-project"
      redirect_path = "/auth/common/callback"
      providers = ["google"]
    }
  }
]
```

第一版规则是：

- 只支持 `confidential`
- 只对声明了 `ignis_login` 的 `http` service 生效
- 浏览器首入口统一走 `IgnisCloud ID` hosted `GET /login`
- control-plane 会把 `IGNIS_LOGIN_CLIENT_ID` / `IGNIS_LOGIN_CLIENT_SECRET` 作为保留 secret 托管
- 当前 igniscloud hosted login 公网地址固定为 `https://id.igniscloud.dev`，不要把 `IGNISCLOUD_ID_BASE_URL` 设计成 env 依赖
- 不支持 `public`
- 不支持直接注入到 `frontend`
- `providers` 当前支持 `google` 和 `test_password`
- `providers` 采用 managed 模式：control-plane 会把远端 app 的 provider 集合收敛到 manifest 声明值
- 正式上线时应只保留正式登录方式；`test_password` 仅用于测试、联调和 smoke test，发布生产配置前应从 manifest 里移除

关于测试登录：

- `test_password` 已经是 `ignis.hcl` 的正式 provider。
- 推荐测试流程是：
  1. 在测试环境的 `ignis.hcl` 里临时声明 `providers = ["google", "test_password"]`，或只声明 `["test_password"]`
  2. 正常 `publish / deploy`，让 control-plane 创建并同步对应的 `IgnisCloud ID` confidential app
  3. 之后 hosted `GET /login` 页面会自动显示测试账号入口，默认账号密码是 `test / testtest`
  4. 正式上线前把 `test_password` 从 `providers` 移除，再重新 `publish / deploy`

### 3.4 引入 `ignis-sdk`

当你的接口不再是单一路径时，推荐引入 `ignis-sdk`：

```toml
[dependencies]
ignis-sdk = { git = "https://github.com/igniscloud/ignis.git", package = "ignis-sdk", tag = "v0.1.1" }
http-body-util = "0.1.3"
wstd = "0.6"
```

默认使用固定的 GitHub `git` 依赖，不要直接猜测 crates.io 版本号，也不要使用浮动 branch。
如果你需要对齐某个未发布提交，改成固定 `rev`。
只有在本地联调 Ignis 仓库时，才改用 `path` 依赖，例如 `ignis-sdk = { path = "../../../../crates/ignis-sdk" }`。

一个最小 Router 写法：

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
        .get("/", |_context: Context| async move {
            text_response(StatusCode::OK, "hello from ignis\n")
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

### 3.5 使用 `ignis.hcl`

`ignis.hcl` 是 Ignis 当前的 project 配置文件。最常见的一份配置如下：

```hcl
project = {
  name = "hello-project"
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
    ignis_login = {
      display_name = "hello-project"
      redirect_path = "/auth/common/callback"
      providers = ["google"]
    }
    env = {
      APP_NAME = "hello-project"
    }
    secrets = {
      OPENAI_API_KEY = "secret://openai-api-key"
    }
    sqlite = {
      enabled = true
    }
    resources = {
      cpu_time_limit_ms = 5000
      memory_limit_bytes = 134217728
    }
  }
]
```

完整字段说明、默认值、校验规则和更多示例见 [ignis.hcl 文档](./ignis-hcl.md)。

这些字段控制：

- project 名称
- service 列表
- service 相对路径
- service 在 project 域名下的路径前缀
- Wasm 构件路径
- 路由基础路径
- 普通环境变量
- secret 映射
- SQLite 能力开关
- 资源限制

### 3.6 发布到兼容控制面

`ignis` 仓库不带公开控制面，但 `ignis-cli` 当前默认会调用托管的 `igniscloud` 控制面：

```bash
ignis login
ignis project create hello-project
cd hello-project
ignis service new --service api --kind http --path services/api
ignis service publish --service api
ignis service deploy --service api <version>
ignis service status --service api
```

这里的 `ignis login` 会打开浏览器，走 igniscloud 登录页，并通过本地 localhost 回调保存 CLI token。

如果平台支持这些接口，你还可以使用：

- `ignis service logs`
- `ignis service env`
- `ignis service secrets`
- `ignis service rollback`
- `ignis service sqlite backup`
- `ignis service sqlite restore`

控制面兼容接口约定见 [API 文档](./api.md)。

## 4. 作为平台开发者接入

### 4.1 最小集成边界

如果你要把 Ignis 嵌进自己的平台，最小接入通常是：

1. 使用 `ignis-manifest` 读取并校验 `ignis.hcl`
2. 从 project 中选中一个 `http` service，并派生运行时需要的 worker manifest
3. 使用 `ignis-runtime` 装载组件并转发 HTTP 请求
4. 使用 `ignis-platform-host` 提供当前的 SQLite host imports
5. 自己实现控制面、认证、发布、版本管理、部署和多节点能力

### 4.2 加载 project manifest

```rust
use ignis_manifest::LoadedProjectManifest;

let loaded = LoadedProjectManifest::load("./ignis.hcl")?;
```

`LoadedProjectManifest` 会：

- 自动解析 `ignis.hcl`
- 校验配置合法性
- 推导 `project_dir`
- 提供 service 查询和 service 目录解析

### 4.3 本地 HTTP 宿主

如果你只需要一个本地宿主进程，可以直接复用：

```rust
use std::net::SocketAddr;

use ignis_manifest::LoadedProjectManifest;
use ignis_runtime::{DevServerConfig, serve};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let project = LoadedProjectManifest::load("./ignis.hcl")?;
    let manifest = project.http_service_manifest("api")?;
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

use ignis_manifest::LoadedProjectManifest;
use ignis_runtime::WorkerRuntime;

let project = LoadedProjectManifest::load("./ignis.hcl")?;
let manifest = project.http_service_manifest("api")?;
let runtime = Arc::new(WorkerRuntime::load(manifest)?);
runtime.warm().await?;
```

这适合：

- 在进程启动时预热组件
- 把 runtime 缓存在你自己的 HTTP server 或 worker manager 中
- 自己管理连接接入、请求分发、实例生命周期

### 4.5 平台宿主扩展点

`ignis-platform-host` 当前暴露了：

- `SqliteHost`
- `HostBindings`

`HostBindings` 是运行时与平台宿主之间的桥接点。默认 `WorkerRuntime` 使用 `SqliteHost`，如果你后续要接入更多平台能力，可以沿这个方向扩展新的宿主类型。

当前 SQLite host 的行为是：

- `ignis.hcl` 中目标 `http` service 的 `sqlite.enabled = true` 时允许访问数据库
- 数据库默认落在该 service 目录下的 `.ignis/sqlite/default.sqlite3`
- 宿主会把 WIT 中定义的 SQLite imports 链接到 Wasmtime 组件里

### 4.6 平台需要自己负责的部分

`ignis` 当前不替你实现：

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
2. 再看 [API 文档](./api.md) 中的 `ignis-sdk` 和 `ignis.hcl`
3. 最后参考 `examples/`

如果你是平台开发者：

1. 先看 [API 文档](./api.md)
2. 再看 `ignis-runtime`、`ignis-platform-host` crate
3. 最后决定自己的控制面接口如何兼容 `ignis-cli`
