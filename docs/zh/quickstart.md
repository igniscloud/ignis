# 快速开始

如果你想从一个空目录尽快跑到第一次部署，直接从这里开始。

## 1. 安装 CLI

macOS / Linux：

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://igniscloud.dev/i.sh | sh
```

Windows PowerShell：

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://igniscloud.dev/i.ps1 | iex"
```

源码安装：

```bash
git clone https://github.com/igniscloud/ignis.git
cd ignis
cargo install --path crates/ignis-cli --force
```

确认 CLI 可用：

```bash
ignis --help
```

## 2. 登录

```bash
ignis login
ignis whoami
```

CLI 会拉起浏览器登录流程，并把 token 保存到本地。

## 3. 创建 project

```bash
ignis project create hello-project
cd hello-project
```

这一步会创建：

- `ignis.hcl`
- `.ignis/project.json`

`ignis.hcl` 保存 project manifest，`.ignis/project.json` 保存远端 `project_id`，用于把本地目录绑定到 control-plane。

## 4. 新增 service

创建 HTTP service：

```bash
ignis service new --service api --kind http --path services/api
```

创建 frontend service：

```bash
ignis service new --service web --kind frontend --path services/web
```

## 5. 检查、构建、发布、部署

HTTP service：

```bash
ignis service check --service api
ignis service build --service api
ignis service publish --service api
ignis service deploy --service api <version>
```

Frontend service：

```bash
ignis service build --service web
ignis service publish --service web
ignis service deploy --service web <version>
```

`<version>` 使用 `ignis service publish` 返回的版本号。

## 6. 生成官方 skill

Codex：

```bash
ignis gen-skill --format codex
```

OpenCode：

```bash
ignis gen-skill --format opencode
```

Raw Markdown：

```bash
ignis gen-skill --format raw
```

## 7. 下一步读什么

- [CLI 工具文档](./cli.md)
- [ignis.hcl 配置文档](./ignis-hcl.md)
- [API 文档](./api.md)
- [Ignis Service Link](./ignis-service-link.md)
