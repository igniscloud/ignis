# CLI 工具文档

本文说明如何使用 `ignis-cli` 创建、构建、调试、发布和维护 Wasm HTTP worker。

`ignis-cli` 当前的二进制名是：

```text
ignis
```

## 1. CLI 能做什么

当前主要覆盖：

- 初始化一个新的 worker 项目
- 本地构建 Wasm 产物
- 本地启动 HTTP 调试服务
- 登录兼容控制面
- 发布、部署、回滚、查询状态
- 管理环境变量、secret 和 SQLite 备份

## 2. 安装与运行

```bash
cargo install --git https://github.com/igniscloud/ignis ignis-cli
```

安装后即可直接执行：

```bash
ignis --help
```

如果你还需要查看源码或 examples：

```bash
git clone https://github.com/igniscloud/ignis.git
cd ignis
```

## 3. 最小工作流

开发一个新 worker，最小链路通常是：

```bash
ignis init hello-worker
cd hello-worker
rustup target add wasm32-wasip2
ignis build
ignis dev --addr 127.0.0.1:3000
```

然后访问：

```bash
curl http://127.0.0.1:3000/
```

## 4. 云端认证

云端命令默认访问：

```text
https://igniscloud.transairobot.com/api
```

CLI 现在支持两种认证方式：

1. 先执行 `ignis login`，走浏览器授权并把 CLI token 持久化到本地 config
2. 每次命令显式传 `--token`，或者通过环境变量注入

登录示例：

```bash
ignis login
ignis whoami
```

登录时 CLI 会：

- 在本机启动一个临时 localhost 回调地址
- 自动打开浏览器，跳到 igniscloud 的登录授权页面
- 用户在浏览器完成 Google/OAuth 登录后，CLI 自动接管返回的 token

退出登录：

```bash
ignis logout
```

临时 token 示例：

```bash
ignis --token <api-token> whoami
IGNIS_TOKEN=<api-token> ignis whoami
```

CLI 读取：

- `IGNIS_TOKEN`
- `IGNISCLOUD_TOKEN`
- `$XDG_CONFIG_HOME/ignis/config.toml`

## 5. 命令说明

### 5.1 `ignis init <path>`

初始化一个新的 worker 项目。

示例：

```bash
ignis init hello-worker
```

默认生成：

- `Cargo.toml`
- `src/lib.rs`
- `wit/world.wit`
- `worker.toml`
- `.gitignore`

如果目录已存在且非空：

```bash
ignis init hello-worker --force
```

### 5.2 `ignis build`

构建当前 worker 的 Wasm 产物。

默认行为：

- 优先尝试 `cargo component build`
- 如果本机没有 `cargo-component`，回退到 `cargo build --target wasm32-wasip2`
- 默认构建 release 产物

示例：

```bash
ignis build
```

指定 manifest：

```bash
ignis build --manifest ./worker.toml
```

### 5.3 `ignis dev`

本地启动一个 HTTP 调试服务，直接加载 Wasm 组件。

示例：

```bash
ignis dev --addr 127.0.0.1:3000
```

如果刚刚已经构建过，也可以跳过构建：

```bash
ignis dev --skip-build --addr 127.0.0.1:3000
```

### 5.4 `ignis whoami`

查看当前 CLI 使用的身份：

```bash
ignis whoami
```

### 5.5 `ignis app`

所有云端 app 维度操作现在统一收敛到 `ignis app ...`：

- `ignis app list`
- `ignis app create`
- `ignis app status`
- `ignis app publish`
- `ignis app deploy`
- `ignis app deployments`
- `ignis app events`
- `ignis app logs`
- `ignis app rollback`
- `ignis app delete-version`
- `ignis app delete`
- `ignis app env ...`
- `ignis app secrets ...`
- `ignis app sqlite ...`

### 5.6 `ignis app create`

创建云端 app：

```bash
ignis app create --app hello-worker
```

如果当前目录有 `worker.toml` 并且已经填写：

```toml
[igniscloud]
app = "hello-worker"
```

也可以直接：

```bash
ignis app create
```

app 名当前限制为：

- 仅允许字母、数字、`-`、`_`
- 最长 48 个字符

### 5.7 `ignis app publish`

上传当前 worker 的 Wasm 产物和 manifest，创建一个新版本。

发布前需要先在 `worker.toml` 里声明目标云端 app：

```toml
[igniscloud]
app = "hello-worker"
```

如果目标 app 不存在，CLI 会提示你创建：

```bash
ignis app publish
```

或者指定 manifest：

```bash
ignis app publish --manifest ./worker.toml
```

### 5.8 `ignis app deploy`

把某个版本切换成当前运行版本。

新写法会优先从 `worker.toml` 读取 `[igniscloud].app`：

```bash
ignis app deploy <version>
```

兼容旧写法：

```bash
ignis app deploy hello-worker <version>
```

也可以显式指定：

```bash
ignis app deploy --app hello-worker <version>
```

### 5.9 `ignis app` 查询命令

- `ignis app list`
- `ignis app status <app>`
- `ignis app deployments <app> --limit <n>`
- `ignis app events <app> --limit <n>`
- `ignis app logs <app> --limit <n>`

示例：

```bash
ignis app list
ignis app status hello-worker
ignis app logs hello-worker --limit 50
```

说明：

- `ignis app status <app>` 会先直接显示该应用的访问地址，再输出完整 JSON
- `ignis app list` 会先列出每个 app 的访问地址，再输出完整 JSON
- 当前公网访问地址格式默认是 `https://<app_id>.transairobot.fun`

示例输出：

```text
$ ignis app status test
URL: https://app-010c8fa84161fd8d.transairobot.fun
{
  "data": {
    "app": "test",
    "app_id": "app-010c8fa84161fd8d",
    "access_url": "https://app-010c8fa84161fd8d.transairobot.fun"
  }
}
```

### 5.10 回滚和删除

回滚：

```bash
ignis app rollback hello-worker <old-version>
```

删除版本：

```bash
ignis app delete-version hello-worker <version>
```

删除应用：

```bash
ignis app delete hello-worker
```

## 6. 环境变量和 Secret

### 6.1 环境变量

查看：

```bash
ignis app env list hello-worker
```

设置：

```bash
ignis app env set hello-worker APP_ENV production
```

删除：

```bash
ignis app env delete hello-worker APP_ENV
```

### 6.2 Secret

查看：

```bash
ignis app secrets list hello-worker
```

设置：

```bash
ignis app secrets set hello-worker openai-api-key <secret-value>
```

删除：

```bash
ignis app secrets delete hello-worker openai-api-key
```

在代码里，secret 通常通过 `worker.toml` 映射成环境变量：

```toml
[secrets]
OPENAI_API_KEY = "secret://openai-api-key"
```

## 7. SQLite 备份与恢复

如果你的 worker 开启了 SQLite，可以通过 CLI 导出和恢复数据库。

备份：

```bash
ignis app sqlite backup hello-worker ./backup.sqlite3
```

恢复：

```bash
ignis app sqlite restore hello-worker ./backup.sqlite3
```

## 8. 组件签名发布

如果你的平台要求组件签名，在执行 `ignis app publish` 之前设置：

- `IGNIS_SIGNING_KEY_ID`
- `IGNIS_SIGNING_KEY_BASE64`

然后正常执行：

```bash
ignis app publish
```

## 9. 最常用的三条链路

### 9.1 写一个本地 HTTP 服务

```bash
ignis init hello-worker
cd hello-worker
rustup target add wasm32-wasip2
ignis build
ignis dev --addr 127.0.0.1:3000
```

### 9.2 发布到控制面

```bash
ignis login
ignis app publish
ignis app deploy hello-worker <version>
ignis app status hello-worker
```

### 9.3 管理一个已上线服务

```bash
ignis app logs hello-worker --limit 100
ignis app env set hello-worker APP_ENV production
ignis app rollback hello-worker <old-version>
```

## 10. 常见问题

### `ignis build` 提示缺少 `wasm32-wasip2`

执行：

```bash
rustup target add wasm32-wasip2
```

### `ignis app publish` 提示找不到 artifact

先执行：

```bash
ignis build
```

并检查 `worker.toml` 里的 `component` 路径是否正确。

### `ignis dev` 启动了但访问不到接口

先确认：

- 监听地址是否正确
- `worker.toml` 中声明的 Wasm 产物是否已经构建成功
- 你的 handler 是否正确匹配请求路径
- 如果使用了 `base_path`，请求路径是否带上了对应前缀

## 11. 与源码对应

如果你需要确认 CLI 文档是否与实现一致，优先查看：

- `crates/ignis-cli/src/main.rs`
- `crates/ignis-cli/src/api.rs`
- `crates/ignis-cli/src/config.rs`
