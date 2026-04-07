---
name: ember
description: Use for Ember worker development and integration with ember-cli, ember-sdk, worker.toml, local dev, compatible control-plane publish/deploy flows, SQLite, and platform host onboarding.
---

# Ember

在当前任务涉及 `ember` worker 的开发、接入、调试、发布或平台嵌入时使用这个 skill。

这个 skill 主要解决三类问题：

- 如何用 `ember` CLI 初始化、构建、本地调试、浏览器登录后发布、部署和排查 worker
- 如何在 worker 项目里接入 `ember-sdk`，实现 HTTP Router、中间件、响应 helper、SQLite 和 migration
- 如何把一个 worker 或运行时接到你自己的平台，包括 `worker.toml`、环境变量、secret、SQLite、兼容控制面 API 和宿主接入

## 快速流程

1. 先读 `references/integration.md`，确认完整接入路径。
2. 如果当前任务偏命令行操作、发布部署或平台配置，继续读 `references/cli.md`。
3. 如果当前任务偏 API 面、manifest、runtime、host 集成或兼容控制面设计，继续读 `references/api.md`。
4. 如果需要仓库总览或公开边界，读 `references/readme.md`。
5. 如果需要最小代码模板，读 `references/hello-worker.rs`；如果要接 SQLite，读 `references/sqlite-worker.rs`。
6. 不要猜测 `worker.toml` 字段、CLI 命令名、兼容控制面接口或 SDK API；以引用文档为准。

## 接入规则

- 最小本地开发链路优先使用：`ember init -> ember build -> ember dev`
- 需要平台交互时，优先走：`ember login -> ember publish -> ember deploy`
- 如果是临时 token 调试或自动化脚本，再显式传 `--token` 或设置 `EMBER_TOKEN`
- 简单 HTTP handler 可直接用 `wstd::http`
- 路由、中间件、统一响应、SQLite 持久化优先用 `ember-sdk`
- 所有 worker 配置优先落在 `worker.toml`，不要在代码里发明额外约定
- 临时调试 CLI token 时，优先用独立环境变量，避免把 token 写进长期环境
- 涉及环境变量、secret、SQLite、发布版本、回滚时，严格按 CLI/API 文档里的命令和字段实现
- 涉及平台嵌入时，优先复用 `ember-manifest`、`ember-runtime`、`ember-platform-host` 的现有边界，不要把控制面逻辑硬塞进 runtime crate

## 典型场景

- 创建一个新的 Rust `wasi:http` worker
- 给现有 worker 接入 `ember_sdk::http::Router`
- 给 worker 打开 SQLite 并加 migration
- 接环境变量、secret 和本地 `ember dev` 调试
- 把本地 worker 发布到兼容控制面并回滚版本
- 设计与 `ember-cli` 兼容的控制面 HTTP API
- 把 `ember-runtime` 和 `ember-platform-host` 嵌入自己的平台宿主
- 排查 `ember whoami`、`ember publish`、`ember deploy`、`ember logs` 相关问题

## 参考资料

- 完整接入流程：`references/integration.md`
- API 说明：`references/api.md`
- CLI 说明：`references/cli.md`
- 仓库总览：`references/readme.md`
- 最小 HTTP 示例：`references/hello-worker.rs`
- SQLite 示例：`references/sqlite-worker.rs`
- 文档索引：`references/doc_index.md`
