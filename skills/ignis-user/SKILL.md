---
name: ignis-user
description: Use for people building Ignis services with ignis-cli, ignis-sdk, ignis.toml, SQLite, secrets, and igniscloud publish/deploy flows.
---

# Ignis User

在当前任务是“使用 Ignis 开发或发布 service”时使用这个 skill，而不是修改 Ignis 仓库本身时使用。

适用范围：

- 使用 `ignis-cli` 初始化、构建、发布和部署 service
- 使用 `ignis-sdk` 的 HTTP Router、中间件、响应 helper、SQLite 和 migration
- 编写或排查 `ignis.toml`
- 使用 igniscloud 兼容流程进行 `login / publish / deploy`

不适用范围：

- 修改 `ignis-cli`、`ignis-sdk`、`ignis-manifest`、`ignis-runtime`、`ignis-platform-host` 源码
- 维护仓库 docs、skills 或生成文档
- 调整 crate 边界、workspace 结构或发布流程

## 快速流程

1. 先读 `references/integration.md`，确认完整接入路径。
2. 如果任务偏 CLI 或发布部署，继续读 `references/cli.md`。
3. 如果任务偏 `ignis.toml` 字段、默认值或示例配置，读 `references/ignis-toml.md`。
4. 如果任务偏 `ignis-sdk` API，用 `references/ignis-sdk/index.md` 作为入口，只继续打开当前需要的模块或 item 页面。
5. 如果 service 配置了 `ignis_login`，或者任务需要接入登录（google 登录），读 `references/igniscloud-id-public-api.md`。
6. 如果需要最小代码模板，读 `references/hello-service.rs`；如果要接 SQLite，读 `references/sqlite-service.rs`。

## 工作规则

- 把 `ignis.toml` 文档和 `ignis-sdk` 生成文档当作配置/API 的事实来源。
- 不要猜测 `ignis.toml` 字段、CLI 命令名、`ignis-sdk` 方法或 secret 约定。
- 没有登录态时，不要把任务卡死在 `ignis project create`；可以手工写 `ignis.toml` 和 service 目录，先完成源码与构建配置。
- 当前推荐工作流是：`ignis login -> ignis project create -> ignis service new -> ignis service build -> ignis service publish -> ignis service deploy`。
- 当前 CLI 不再把本地 `dev` 作为主工作流；默认以构建、发布、部署为准，后续再扩测试环境部署能力。
- 简单 handler 可以直接用 `wstd::http`，但多路由、中间件、统一响应、SQLite 通常优先用 `ignis-sdk`。
- 需要查 SDK 细节时，优先读 `mddoc` 生成的单页，不要只靠摘要文档推断。
- 当前公网路由模型是一个 project host 下按 path prefix 暴露 services，例如前端走 `/`，API 走 `/api`，不要再假设 `api.<project-host>` 这类子域。
- 如果某个 `http` service 声明了 `ignis_login`，当前只允许 `providers = ["google"]`。
- 浏览器登录首入口优先走 `IgnisCloud ID` hosted `GET /login`，不要直接假设业务 app 自己拉起 Google。
- 如果 manifest 里出现 `ignis_login`，先读 `references/igniscloud-id-public-api.md`，再决定回调路径、登录入口和后端换码方式。
- 当前 `http` service 统一使用标准 `wasm32-wasip2` 构建路径，不要再按 `cargo-component` 工作流推断 CLI 行为。
- `frontend` service 的本地静态预览能力不是主工作流，也不要假设它自动提供 SPA fallback。
- `ignis-sdk` 依赖来源不要猜测；如果当前版本未发布到 crates.io，使用明确的 `path` 或固定 `git` 版本。

## 典型场景

- 创建一个新的 Rust `wasi:http` service
- 给现有 service 接入 `ignis_sdk::http::Router`
- 配置 `ignis.toml` 里的 prefix、env、secret、SQLite、network、igniscloud
- 给 service 增加 SQLite migration
- 构建、发布、部署一个 service
- 发布、部署、回滚和排查 igniscloud 兼容链路

## 参考资料

- 接入流程：`references/integration.md`
- CLI：`references/cli.md`
- `ignis.toml`：`references/ignis-toml.md`
- `IgnisCloud ID` Public API：`references/igniscloud-id-public-api.md`
- `ignis-sdk` 生成文档入口：`references/ignis-sdk/index.md`
- 仓库首页：`references/readme.md`
- 最小 HTTP 示例：`references/hello-service.rs`
- SQLite 示例：`references/sqlite-service.rs`
- 文档索引：`references/doc_index.md`
