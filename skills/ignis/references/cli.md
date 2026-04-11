# CLI 工具文档

本文说明如何使用 `ignis-cli` 创建 project、创建 service、构建产物，并发布到兼容的 igniscloud control-plane。

`ignis-cli` 当前的二进制名是：

```text
ignis
```

## 1. CLI 能做什么

当前主要覆盖：

- 浏览器登录和本地 token 持久化
- 生成 `ignis` 的 `codex`、`opencode`、`raw` 三种 skill 包
- 创建云端 project，并初始化本地 project 根目录
- 在 project 下创建 `http` 或 `frontend` service
- 检查本地 service manifest 的常见配置异常
- 构建单个 service，并发布/部署到云端
- 发布、部署、回滚、查询 service 状态
- 管理 service 级 env、secret 和 SQLite 备份

## 2. 安装与认证

安装：

```bash
cargo install --git https://github.com/igniscloud/ignis ignis-cli
```

查看帮助：

```bash
ignis --help
```

登录：

```bash
ignis login
ignis whoami
```

CLI 会：

- 在本机启动临时 localhost 回调
- 打开浏览器跳到 igniscloud 登录页
- 同时在终端输出登录 URL，方便手动复制打开
- 授权完成后把 token 保存到本地配置

退出登录：

```bash
ignis logout
```

也可以显式传 token：

```bash
ignis --token <token> whoami
IGNIS_TOKEN=<token> ignis whoami
```

CLI 会读取：

- `IGNIS_TOKEN`
- `IGNISCLOUD_TOKEN`
- `$XDG_CONFIG_HOME/ignis/config.toml`

## 3. 配置文件

Ignis 当前使用 project 级配置文件：

```text
ignis.hcl
```

这个文件位于 project 根目录。所有 `ignis service ...` 命令都会从当前目录向上查找 `ignis.hcl`。

除了 `ignis.hcl` 之外，CLI 还会在 project 根目录写入本地状态文件：

```text
.ignis/project.json
```

这个文件保存 control-plane 返回的 `project_id`，用于把本地 project 绑定到远端唯一标识。`project.name` 继续保留为展示名，不再作为远端写操作的默认覆盖键。

同时，`ignis.hcl` 里的 `project.domain` 会保存当前线上访问域名：

- 没有自定义域名时，值通常是默认域名 `<project_id>.<base-domain>`
- 绑定了自定义域名后，值会切到当前自定义域名
- 这个字段由 CLI 在 `project create`、`project sync --mode apply`、`domain create`、`domain delete` 时自动维护

需要特别区分两类标识：

- `project.name`
  写在 `ignis.hcl` 的展示名，也是 `ignis project create <name>` 里的输入
- `project_id`
  由 control-plane 在创建远端 project 时分配，并写入 `.ignis/project.json`

当前 CLI 的 `ignis service new`、`publish`、`deploy`、`env`、`secrets`、`sqlite` 等远端 service 操作，都会使用 `.ignis/project.json` 里的 `project_id`，不再按 `project.name` 猜测或回退命中远端项目。

完整配置说明见 [ignis.hcl 文档](./ignis-hcl.md)。

## 4. 最小工作流

一个最小 `http` service 工作流：

```bash
ignis login
ignis project create hello-project
cd hello-project
ignis service new --service api --kind http --path services/api
ignis project sync --mode plan
ignis project sync --mode apply
ignis service check --service api
ignis service build --service api
ignis service publish --service api
ignis service deploy --service api <version>
```

说明：

- `ignis project create` 会立即创建远端 project，并把返回的 `project_id` 写入 `.ignis/project.json`
- `ignis project create` 还会查询当前线上域名，并把它写入 `ignis.hcl` 的 `project.domain`
- 如果这个 project 是从 Git 拉下来的、目录里只有 `ignis.hcl` 而没有 `.ignis/project.json`，先执行 `ignis project sync --mode apply` 完成远端绑定，再执行任何远端 service 操作
- `ignis service deploy --service api <version>` 里的 `<version>` 来自前一步 `ignis service publish --service api` 的返回结果

如果你要创建前端 service：

```bash
ignis service new --service web --kind frontend --path services/web
ignis service build --service web
ignis service publish --service web
ignis service deploy --service web <version>
```

## 4.1 `ignis gen-skill`

一级命令：

- `ignis gen-skill --format <codex|opencode|raw>`

当前会一次生成两个官方 skill：

- `ignis`
- `ignis-login`

支持三种格式：

- `codex`
  默认输出到 `.codex/skills/<skill>/SKILL.md`
- `opencode`
  默认输出到 `.opencode/skills/<skill>/SKILL.md`
- `raw`
  默认输出到 `./<skill>/skill.md`

示例：

```bash
ignis gen-skill --format codex
```

```bash
ignis gen-skill --format opencode
```

```bash
ignis gen-skill --format raw
```

三种格式都会带上两个 skill 各自依赖的 `references/` 文档，生成后可以脱离当前仓库单独使用。

也可以显式指定输出根目录：

```bash
ignis gen-skill --format codex --path ./internal-skills
```

这样会生成：

- `./internal-skills/ignis/SKILL.md`
- `./internal-skills/ignis-login/SKILL.md`

如果目标 skill 目录已经存在，需要显式传 `--force` 覆盖。

## 5. `ignis project`

一级命令：

- `ignis project create <name>`
- `ignis project sync --mode <plan|apply>`
- `ignis project list`
- `ignis project status <name>`
- `ignis project delete <name>`
- `ignis project token create <name>`
- `ignis project token revoke <name> <token-id>`

### 5.1 `ignis project create <name>`

创建云端 project，并在本地初始化 project 根目录。

示例：

```bash
ignis project create hello-project
```

默认会：

- 在 control-plane 创建 project
- 创建本地目录 `./hello-project`
- 写入空的 `ignis.hcl`
- 写入 `.ignis/project.json`，保存当前 project 的 `project_id`
- 查询当前线上域名，并写入 `ignis.hcl` 的 `project.domain`

如果 control-plane 的创建响应没有返回 `project_id`，CLI 仍会创建本地目录和 `ignis.hcl`，但后续 `ignis service ...` 远端操作会因为缺少绑定而失败。这种情况下先修复 control-plane 响应，再重新执行 `ignis project sync --mode apply`。

也可以指定目录：

```bash
ignis project create hello-project --dir ./demo
```

如果目录已存在且非空，需要显式传 `--force`。

### 5.2 `ignis project token create`

创建 project 级 token：

```bash
ignis project token create hello-project
```

这个 token 可以管理该 project 下的全部 services。

### 5.3 `ignis project sync`

在本地已有 `ignis.hcl` 的 project 目录里，先生成同步计划，再按需把 project 和缺失的 services 应用到云端。

示例：

```bash
cd hello-project
ignis project sync
ignis project sync --mode plan
ignis project sync --mode apply
```

当前行为：

- 默认 `--mode plan`，只计算同步计划，不做远端写操作
- 如果本地还没有 `.ignis/project.json`，`plan` 会把当前 project 视为“未绑定远端项目”，不会再按 `project.name` 静默查找同名远端 project
- `plan` 会列出 project / service 级动作，并对 manifest drift 输出字段级 diff
- `apply` 只执行当前安全支持的动作：创建缺失的 project 和 service；首次创建成功后会把返回的 `project_id` 写入 `.ignis/project.json`
- 如果本地缺少 `project.domain`，`apply` 会把线上当前域名写回 `ignis.hcl`
- 如果本地 `project.domain` 和线上当前域名不一致，`sync` 会直接失败
- 如果云端 service 已存在但 manifest 不一致，当前会生成 `repair_service_manifest` 计划项并标记为 `blocked`

说明：

- 当前 control-plane API 还没有 service manifest update 接口，所以 drift 已经可审阅，但还不会在 `apply` 里被自动修复
- `sync` 执行前会先运行本地 service 检查；如果存在 `error` 级问题，会直接失败

### 5.4 `ignis domain`

一级命令：

- `ignis domain list <project>`
- `ignis domain create <project> <label>`
- `ignis domain delete <project> <label>`

示例：

```bash
ignis domain list hello-project
ignis domain create hello-project helloexample
ignis domain delete hello-project helloexample
```

当前行为：

- `list` 会返回默认域名、自定义域名、当前价格和剩余额度
- `create` 只需要传子域名前缀 `label`；平台会生成完整 host
- `delete` 删除的是当前 project 上的自定义域名，不影响默认域名
- 如果命令是在目标 project 目录里执行，CLI 还会同步更新 `ignis.hcl` 的 `project.domain`

## 6. `ignis service`

一级命令：

- `ignis service new --service <name> --kind <http|frontend> --path <relative-path>`
- `ignis service list`
- `ignis service status --service <name>`
- `ignis service check --service <name>`
- `ignis service delete --service <name>`
- `ignis service build --service <name>`
- `ignis service publish --service <name>`
- `ignis service deploy --service <name> <version>`
- `ignis service deployments --service <name>`
- `ignis service events --service <name>`
- `ignis service logs --service <name>`
- `ignis service rollback --service <name> <version>`
- `ignis service delete-version --service <name> <version>`
- `ignis service env ...`
- `ignis service secrets ...`
- `ignis service sqlite ...`

约束：

- 所有 `service` 命令都必须在 project 目录内执行
- CLI 会从当前目录向上查找 `ignis.hcl`
- 除 `service list` 外，所有操作都必须显式指定 `--service`

### 6.1 `ignis service new`

同时创建本地 service 和云端 service。

前提：

- 当前 project 已经通过 `.ignis/project.json` 绑定到远端 `project_id`
- 如果是旧 project 或手工创建的本地目录，先执行 `ignis project sync --mode apply`

示例：

```bash
ignis service new --service api --kind http --path services/api
```

或者：

```bash
ignis service new --service web --kind frontend --path services/web
```

执行时会：

1. 读取当前 project 的 `ignis.hcl`
2. 校验 service 名不能重复
3. 校验 `path` 是相对路径
4. 校验 `path` 没有和已有 service 冲突
5. 拒绝写入已存在且非空的目录
6. 先创建云端 service
7. 再更新本地 `ignis.hcl`
8. 生成模板文件

`http` 模板会生成：

- `Cargo.toml`
- `src/lib.rs`
- `wit/world.wit`
- `.gitignore`

`frontend` 模板会生成：

- `src/index.html`
- `.gitignore`

### 6.2 `ignis service delete`

删除云端 service，并从本地 `ignis.hcl` 里移除该 service 条目：

```bash
ignis service delete --service api
```

说明：

- 如果该 service 还有 active deployment，control-plane 会拒绝删除
- CLI 不会自动删除本地 service 目录

### 6.3 `ignis service check`

检查当前本地 `ignis.hcl` 里的单个 service，输出常见配置异常。

示例：

```bash
ignis service check --service api
```

当前会检查：

- `ignis_login` service 是否错误地把 `IGNISCLOUD_ID_BASE_URL` 作为 env 依赖
- `ignis_login` service 是否允许访问 `id.igniscloud.dev`

如果发现 `error` 级问题，命令会返回非零退出码。

`ignis service publish` 也会在真正上传构件前自动执行同一套本地检查。

### 6.4 `ignis service list`

列出当前 project 在 `ignis.hcl` 中声明的 services：

```bash
ignis service list
```

### 6.5 `ignis service build`

构建单个 service。

`http` service：

```bash
ignis service build --service api
```

行为：

- 统一执行 `cargo build --target wasm32-wasip2`

`frontend` service：

```bash
ignis service build --service web
```

行为：

- 在 service 目录执行 `frontend.build_command`

### 6.6 `ignis service publish`

发布当前 service 的新版本。

```bash
ignis service publish --service api
```

对于 `http` service，CLI 会上传：

- `service_manifest`
- `build_metadata`
- `component_sha256`
- `component`
- 可选 `signature`

对于 `frontend` service，CLI 会上传：

- `service_manifest`
- `build_metadata`
- `site_bundle`

### 6.7 `ignis service deploy`

把某个版本部署成当前运行版本：

```bash
ignis service deploy --service api <version>
```

### 6.8 `ignis service status`

查看 service 当前状态：

```bash
ignis service status --service api
```

### 6.9 查询命令

部署历史：

```bash
ignis service deployments --service api --limit 20
```

事件：

```bash
ignis service events --service api --limit 20
```

日志：

```bash
ignis service logs --service api --limit 100
```

回滚：

```bash
ignis service rollback --service api <version>
```

删除版本：

```bash
ignis service delete-version --service api <version>
```

## 7. Env、Secrets、SQLite

环境变量：

```bash
ignis service env list --service api
ignis service env set --service api LOG_LEVEL debug
ignis service env delete --service api LOG_LEVEL
```

Secrets：

```bash
ignis service secrets list --service api
ignis service secrets set --service api OPENAI_API_KEY secret://openai
ignis service secrets delete --service api OPENAI_API_KEY
```

SQLite：

```bash
ignis service sqlite backup --service api ./backup.sqlite3
ignis service sqlite restore --service api ./backup.sqlite3
```

## 8. 组件签名

`http` service 发布时支持对 Wasm 组件签名。

环境变量：

- `IGNIS_SIGNING_KEY_ID`
- `IGNIS_SIGNING_KEY_BASE64`

签名算法：

- Ed25519

签名对象：

- 发布时上传的 Wasm 组件字节流

## 9. 常见问题

### 9.1 找不到 `ignis.hcl`

如果你在执行 `ignis service ...` 时看到找不到 `ignis.hcl`：

- 确认当前目录位于 project 根目录或其子目录
- 确认已经先执行过 `ignis project create ...`
- 确认 `ignis.hcl` 没有被删掉

### 9.2 `service new` 报路径冲突

`service new` 会拒绝：

- 与已有 service 复用同一路径
- 写入已存在且非空的目录

这种情况下，换一个新的 `--path`，或者先清理本地目录。

### 9.3 project 未绑定远端 `project_id`

如果你在执行 `ignis service new`、`publish`、`deploy`、`env`、`secrets`、`sqlite` 等远端操作时看到 `project_not_linked`：

- 在 project 根目录执行 `ignis project sync --mode apply`
- 确认 CLI 已经写入 `.ignis/project.json`
- 不要再依赖 `project.name` 命中远端 project；同名 project 现在不会被自动复用

### 9.4 `service publish` / `deploy` 返回 `404 project <id> not found`

如果 `.ignis/project.json` 已经存在，且里面也有 `project_id`，但 `ignis service publish` 或 `ignis service deploy` 仍然返回这类错误：

- 先执行 `ignis project sync --mode plan` 或 `ignis project sync --mode apply`，确认 CLI 读取到的绑定没有丢
- 确认 `ignis project create` 或 `sync --mode apply` 的响应里确实返回了 `data.project_id` 或 `data.id`
- 这通常说明 control-plane 的 service 相关接口没有正确识别 CLI 传入的 `project_id`，而不是本地少执行了某个初始化步骤
- 不要尝试通过手工把 `project.name` 填回 `.ignis/project.json` 规避问题；那会重新引入同名 project 误命中的风险

### 9.5 `service publish` 找不到产物

先确认：

- `http` service 的 `services.http.component` 路径是否正确
- `frontend` service 的 `services.frontend.output_dir` 是否存在
- 是否已经先执行 `ignis service build --service <name>`

### 9.6 `project.domain` 和线上域名不一致

如果 `ignis project sync --mode plan` 或 `apply` 报 `project_domain_mismatch`：

- 先执行 `ignis domain list <project>` 看线上当前域名
- 把 `ignis.hcl` 里的 `project.domain` 改成线上当前值
- 或者在 project 目录里执行一次 `ignis domain create ...` / `ignis domain delete ...`，让 CLI 自动回写本地配置

## 10. 相关文档

- [接入文档](./integration.md)
- [API 文档](./api.md)
- [ignis.hcl 文档](./ignis-hcl.md)
