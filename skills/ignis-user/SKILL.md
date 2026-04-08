---
name: ignis-user
description: Use for people building Ignis services with ignis-cli, ignis-sdk, ignis.toml, local dev, SQLite, secrets, and igniscloud publish/deploy flows.
---

# Ignis User

在当前任务是“使用 Ignis 开发或发布 worker”时使用这个 skill，而不是修改 Ignis 仓库本身时使用。

适用范围：

- 使用 `ignis-cli` 初始化、构建、本地调试和发布 worker
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
5. 如果需要最小代码模板，读 `references/hello-worker.rs`；如果要接 SQLite，读 `references/sqlite-worker.rs`。

## 工作规则

- 把 `worker.toml` 文档和 `ignis-sdk` 生成文档当作配置/API 的事实来源。
- 不要猜测 `ignis.toml` 字段、CLI 命令名、`ignis-sdk` 方法或 secret 约定。
- 本地开发优先使用：`ignis project create -> ignis service new -> ignis service build -> ignis service dev`。
- 云端交互优先使用：`ignis login -> ignis service publish -> ignis service deploy`。
- 简单 handler 可以直接用 `wstd::http`，但多路由、中间件、统一响应、SQLite 通常优先用 `ignis-sdk`。
- 需要查 SDK 细节时，优先读 `mddoc` 生成的单页，不要只靠摘要文档推断。
- 当前公网路由模型是一个 project host 下按 path prefix 暴露 services，例如前端走 `/`，API 走 `/api`，不要再假设 `api.<project-host>` 这类子域。

## 典型场景

- 创建一个新的 Rust `wasi:http` worker
- 给现有 worker 接入 `ignis_sdk::http::Router`
- 配置 `ignis.toml` 里的 prefix、env、secret、SQLite、network、igniscloud
- 给 worker 增加 SQLite migration
- 本地调试 `ignis dev`
- 发布、部署、回滚和排查 igniscloud 兼容链路

## 参考资料

- 接入流程：`references/integration.md`
- CLI：`references/cli.md`
- `ignis.toml`：`references/ignis-toml.md`
- `ignis-sdk` 生成文档入口：`references/ignis-sdk/index.md`
- 仓库首页：`references/readme.md`
- 最小 HTTP 示例：`references/hello-worker.rs`
- SQLite 示例：`references/sqlite-worker.rs`
- 文档索引：`references/doc_index.md`
