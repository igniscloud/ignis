---
name: ignis
description: Use for building and operating Ignis projects with ignis-cli, ignis-sdk, ignis.hcl, SQLite, async jobs, cron schedules, platform object-store presign, service build/publish/deploy, and example-driven project setup.
---

# Ignis

在当前任务是“使用 Ignis 开发或发布 service”时使用这个 skill，而不是修改 Ignis 仓库本身时使用。

- 使用 `ignis` 初始化、构建、发布和部署 service
- 使用 `ignis-sdk` 的 HTTP Router、中间件、响应 helper、SQLite、migration 和 object-store presign
- 编写或排查 `ignis.hcl`，包括 services、jobs 和 schedules
- 使用 examples 里的最小项目结构快速起步
- 你需要先查看ignis是否安装，若为安装就阅读cli文档安装



## 快速流程

1. 先读 `references/integration.md`，确认完整接入路径。
2. 如果任务偏 CLI 或发布部署，继续读 `references/cli.md`。
3. 如果任务偏 `ignis.hcl` 字段、默认值或示例配置，读 `references/ignis-hcl.md`。
4. 如果任务偏 `ignis-sdk` API，用 `references/ignis-sdk/index.md` 作为入口，只继续打开当前需要的模块或 item 页面。
5. 如果任务涉及 async jobs、manual job API、cron schedules、job runs 或 job execution headers，读 `references/jobs-and-schedules.md`。
6. 如果任务涉及平台托管 COS/S3 presigned URL，读 `references/object-store-presign.md`；需要完整上传例子时读 `references/examples/cos-and-jobs-example/`。
7. 如果任务涉及登录或 `[services.ignis_login]`，切到 `ignis-login` skill。
8. 如果需要最小 HTTP / SQLite 模板，优先读整个 example 项目：
   `references/examples/hello-fullstack/` 和 `references/examples/sqlite-example/`。

## 工作规则

- 把 `ignis.hcl` 文档和 `ignis-sdk` 生成文档当作配置/API 的事实来源。
- 不要猜测 `ignis.hcl` 字段、CLI 命令名、`ignis-sdk` 方法或 secret 约定。
- 当前推荐工作流是：`ignis login -> ignis project create -> ignis service new -> ignis service build -> ignis service publish -> ignis service deploy`。
- 当前 CLI 不再把本地 `dev` 作为主工作流；默认以构建、发布、部署为准。
- 简单 handler 可以直接用 `wstd::http`，但多路由、中间件、统一响应、SQLite 通常优先用 `ignis-sdk`。
- 需要查 SDK 细节时，优先读 `mddoc` 生成的单页，不要只靠摘要文档推断。
- 当前公网路由模型是一个 project host 下按 path prefix 暴露 services，例如前端走 `/`，API 走 `/api`，不要再假设 `api.<project-host>` 这类子域。
- 当前 `http` service 统一使用标准 `wasm32-wasip2` 构建路径，不要再按 `cargo-component` 工作流推断 CLI 行为。
- `ignis-sdk` 依赖来源不要猜测；默认给用户 GitHub Cargo 依赖写法，例如 `ignis-sdk = { git = "https://github.com/igniscloud/ignis.git", package = "ignis-sdk", tag = "v0.1.1" }`。只有在本地联调 Ignis 仓库时再改用明确的 `path`。
- 平台托管对象存储优先使用 presign：Wasm service 调 `ignis_sdk::object_store`，host/control-plane 完成签名，不要把 COS/S3 AK/SK 暴露给 Wasm 或浏览器。
- Jobs/schedules 是 project automation：在 `ignis.hcl` 顶层声明 `jobs` / `schedules` 后通过 `ignis project sync --mode apply` 同步；job target 走同项目 HTTP service，不要写任意外部 URL。

## 参考资料

- 接入流程：`references/integration.md`
- CLI：`references/cli.md`
- `ignis.hcl`：`references/ignis-hcl.md`
- `ignis-sdk` 生成文档入口：`references/ignis-sdk/index.md`
- Jobs and schedules：`references/jobs-and-schedules.md`
- Object-store presign：`references/object-store-presign.md`
- 最小 HTTP 示例项目：`references/examples/hello-fullstack/`
- SQLite 示例项目：`references/examples/sqlite-example/`
- COS 上传和jobs，corn使用方式完整示例：`references/examples/cos-and-jobs-example/`
- 文档索引：`references/doc_index.md`
