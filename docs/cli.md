# CLI 工具文档

本文说明如何使用 `ignis-cli` 创建 project、创建 service、构建产物，并发布到兼容的 igniscloud control-plane。

`ignis-cli` 当前的二进制名是：

```text
ignis
```

## 1. CLI 能做什么

当前主要覆盖：

- 浏览器登录和本地 token 持久化
- 生成 `ignis-user` 的 `codex`、`opencode`、`raw` 三种 skill 包
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
ignis.toml
```

这个文件位于 project 根目录。所有 `ignis service ...` 命令都会从当前目录向上查找 `ignis.toml`。

完整配置说明见 [ignis.toml 文档](./ignis-toml.md)。

## 4. 最小工作流

一个最小 `http` service 工作流：

```bash
ignis login
ignis project create hello-project
cd hello-project
ignis project sync
ignis service new --service api --kind http --path services/api
ignis service check --service api
ignis service build --service api
ignis service publish --service api
ignis service deploy --service api <version>
```

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

当前支持三种格式：

- `codex`
  默认输出到 `.codex/skills/ignis-user/SKILL.md`
- `opencode`
  默认输出到 `.opencode/skills/ignis-user/SKILL.md`
- `raw`
  默认输出到 `ignis-user/skill.md`

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

三种格式都会带上 `ignis-user` 依赖的 `references/` 文档，生成后可以脱离当前仓库单独使用。

也可以显式指定输出目录：

```bash
ignis gen-skill --format codex --path ./internal-skills/ignis-user
```

如果目标已经存在，需要显式传 `--force` 覆盖。

## 5. `ignis project`

一级命令：

- `ignis project create <name>`
- `ignis project sync`
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
- 写入空的 `ignis.toml`

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

在本地已有 `ignis.toml` 的 project 目录里，把 project 和缺失的 services 同步到云端。

示例：

```bash
cd hello-project
ignis project sync
```

当前行为：

- 如果云端 project 不存在，先创建 project
- 如果某个本地 service 还没在云端创建，自动创建该 service
- 如果云端 service 已存在且 manifest 一致，标记为 `unchanged`
- 如果云端 service 已存在但 manifest 不一致，标记为 `drift`

说明：

- 当前 `sync` 只创建缺失资源，不会覆盖已存在的云端 service manifest
- `sync` 执行前会先运行本地 service 检查；如果存在 `error` 级问题，会直接失败

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
- CLI 会从当前目录向上查找 `ignis.toml`
- 除 `service list` 外，所有操作都必须显式指定 `--service`

### 6.1 `ignis service new`

同时创建本地 service 和云端 service。

示例：

```bash
ignis service new --service api --kind http --path services/api
```

或者：

```bash
ignis service new --service web --kind frontend --path services/web
```

### 6.2 `ignis service delete`

删除云端 service，并从本地 `ignis.toml` 里移除该 service 条目：

```bash
ignis service delete --service api
```

说明：

- 如果该 service 还有 active deployment，control-plane 会拒绝删除
- CLI 不会自动删除本地 service 目录

执行时会：

1. 读取当前 project 的 `ignis.toml`
2. 校验 service 名不能重复
3. 校验 `path` 是相对路径
4. 校验 `path` 没有和已有 service 冲突
5. 拒绝写入已存在且非空的目录
6. 先创建云端 service
7. 再更新本地 `ignis.toml`
8. 生成模板文件

`http` 模板会生成：

- `Cargo.toml`
- `src/lib.rs`
- `wit/world.wit`
- `.gitignore`

### 6.3 `ignis service check`

检查当前本地 `ignis.toml` 里的单个 service，输出常见配置异常。

示例：

```bash
ignis service check --service api
```

当前会检查：

- `ignis_login` service 是否错误地把 `IGNISCLOUD_ID_BASE_URL` 作为 env 依赖
- `ignis_login` service 是否允许访问 `id.igniscloud.transairobot.com`

如果发现 `error` 级问题，命令会返回非零退出码。

`ignis service publish` 也会在真正上传构件前自动执行同一套本地检查。

`frontend` 模板会生成：

- `src/index.html`
- `.gitignore`

### 6.2 `ignis service list`

列出当前 project 在 `ignis.toml` 中声明的 services：

```bash
ignis service list
```

### 6.3 `ignis service build`

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

### 6.4 `ignis service publish`

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

### 6.5 `ignis service deploy`

把某个版本部署成当前运行版本：

```bash
ignis service deploy --service api <version>
```

### 6.6 `ignis service status`

查看 service 当前状态：

```bash
ignis service status --service api
```

### 6.7 查询命令

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

### 9.1 找不到 `ignis.toml`

如果你在执行 `ignis service ...` 时看到找不到 `ignis.toml`：

- 确认当前目录位于 project 根目录或其子目录
- 确认已经先执行过 `ignis project create ...`
- 确认 `ignis.toml` 没有被删掉

### 9.2 `service new` 报路径冲突

`service new` 会拒绝：

- 与已有 service 复用同一路径
- 写入已存在且非空的目录

这种情况下，换一个新的 `--path`，或者先清理本地目录。

### 9.3 `service publish` 找不到产物

先确认：

- `http` service 的 `services.http.component` 路径是否正确
- `frontend` service 的 `services.frontend.output_dir` 是否存在
- 是否已经先执行 `ignis service build --service <name>`

## 10. 相关文档

- [接入文档](./integration.md)
- [API 文档](./api.md)
- [ignis.toml 文档](./ignis-toml.md)
