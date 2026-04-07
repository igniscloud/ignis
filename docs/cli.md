# CLI 工具文档

本文说明如何使用 `ember-cli` 创建、构建、调试、发布和维护 Wasm HTTP worker。

`ember-cli` 当前的二进制名是：

```text
ember
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
cargo install --git https://github.com/pleasewhy/ember ember-cli
```

安装后即可直接执行：

```bash
ember --help
```

如果你还需要查看源码或 examples：

```bash
git clone https://github.com/pleasewhy/ember.git
cd ember
```

## 3. 最小工作流

开发一个新 worker，最小链路通常是：

```bash
ember init hello-worker
cd hello-worker
rustup target add wasm32-wasip2
ember build
ember dev --addr 127.0.0.1:3000
```

然后访问：

```bash
curl http://127.0.0.1:3000/
```

## 4. 云端认证

云端命令默认访问：

```text
https://embercloud.transairobot.com/api
```

CLI 现在支持两种认证方式：

1. 先执行 `ember login`，走浏览器授权并把 CLI token 持久化到本地 config
2. 每次命令显式传 `--token`，或者通过环境变量注入

登录示例：

```bash
ember login
ember whoami
```

登录时 CLI 会：

- 在本机启动一个临时 localhost 回调地址
- 自动打开浏览器，跳到 embercloud 的登录授权页面
- 用户在浏览器完成 Google/OAuth 登录后，CLI 自动接管返回的 token

退出登录：

```bash
ember logout
```

临时 token 示例：

```bash
ember --token <api-token> whoami
EMBER_TOKEN=<api-token> ember whoami
```

为了兼容旧链路，CLI 仍会回退读取：

- `EMBERCLOUD_TOKEN`
- `WKR_API_TOKEN`
- `$XDG_CONFIG_HOME/ember/config.toml`
- `$XDG_CONFIG_HOME/embercloud/config.toml`
- `$XDG_CONFIG_HOME/wkr/config.toml`

## 5. 命令说明

### 5.1 `ember init <path>`

初始化一个新的 worker 项目。

示例：

```bash
ember init hello-worker
```

默认生成：

- `Cargo.toml`
- `src/lib.rs`
- `wit/world.wit`
- `worker.toml`
- `.gitignore`

如果目录已存在且非空：

```bash
ember init hello-worker --force
```

### 5.2 `ember build`

构建当前 worker 的 Wasm 产物。

默认行为：

- 优先尝试 `cargo component build`
- 如果本机没有 `cargo-component`，回退到 `cargo build --target wasm32-wasip2`
- 默认构建 release 产物

示例：

```bash
ember build
```

指定 manifest：

```bash
ember build --manifest ./worker.toml
```

### 5.3 `ember dev`

本地启动一个 HTTP 调试服务，直接加载 Wasm 组件。

示例：

```bash
ember dev --addr 127.0.0.1:3000
```

如果刚刚已经构建过，也可以跳过构建：

```bash
ember dev --skip-build --addr 127.0.0.1:3000
```

### 5.4 `ember whoami`

查看当前 CLI 使用的身份：

```bash
ember whoami
```

### 5.5 `ember create-app`

创建云端 app：

```bash
ember create-app --app hello-worker
```

如果当前目录有 `worker.toml` 并且已经填写：

```toml
[embercloud]
app = "hello-worker"
```

也可以直接：

```bash
ember create-app
```

app 名当前限制为：

- 仅允许字母、数字、`-`、`_`
- 最长 48 个字符

### 5.6 `ember publish`

上传当前 worker 的 Wasm 产物和 manifest，创建一个新版本。

发布前需要先在 `worker.toml` 里声明目标云端 app：

```toml
[embercloud]
app = "hello-worker"
```

如果目标 app 不存在，CLI 会提示你创建：

```bash
ember publish
```

或者指定 manifest：

```bash
ember publish --manifest ./worker.toml
```

### 5.7 `ember deploy`

把某个版本切换成当前运行版本。

新写法会优先从 `worker.toml` 读取 `[embercloud].app`：

```bash
ember deploy <version>
```

兼容旧写法：

```bash
ember deploy hello-worker <version>
```

也可以显式指定：

```bash
ember deploy --app hello-worker <version>
```

### 5.8 查询命令

平台级查询：

- `ember apps`
- `ember nodes`

单应用查询：

- `ember status <app>`
- `ember deployments <app> --limit <n>`
- `ember events <app> --limit <n>`
- `ember logs <app> --limit <n>`

示例：

```bash
ember apps
ember status hello-worker
ember logs hello-worker --limit 50
```

说明：

- `ember status <app>` 会先直接显示该应用的访问地址，再输出完整 JSON
- `ember apps` 会先列出每个 app 的访问地址，再输出完整 JSON
- 当前公网访问地址格式默认是 `https://<app_id>.transairobot.fun`

示例输出：

```text
$ ember status test
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
ember rollback hello-worker <old-version>
```

删除版本：

```bash
ember delete-version hello-worker <version>
```

删除应用：

```bash
ember delete-app hello-worker
```

## 6. 环境变量和 Secret

### 6.1 环境变量

查看：

```bash
ember env list hello-worker
```

设置：

```bash
ember env set hello-worker APP_ENV production
```

删除：

```bash
ember env delete hello-worker APP_ENV
```

### 6.2 Secret

查看：

```bash
ember secrets list hello-worker
```

设置：

```bash
ember secrets set hello-worker openai-api-key <secret-value>
```

删除：

```bash
ember secrets delete hello-worker openai-api-key
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
ember sqlite backup hello-worker ./backup.sqlite3
```

恢复：

```bash
ember sqlite restore hello-worker ./backup.sqlite3
```

## 8. 组件签名发布

如果你的平台要求组件签名，在执行 `ember publish` 之前设置：

- `EMBER_SIGNING_KEY_ID`
- `EMBER_SIGNING_KEY_BASE64`

为了兼容旧链路，CLI 当前也接受：

- `WKR_SIGNING_KEY_ID`
- `WKR_SIGNING_KEY_BASE64`

然后正常执行：

```bash
ember publish
```

## 9. 最常用的三条链路

### 9.1 写一个本地 HTTP 服务

```bash
ember init hello-worker
cd hello-worker
rustup target add wasm32-wasip2
ember build
ember dev --addr 127.0.0.1:3000
```

### 9.2 发布到控制面

```bash
ember login
ember publish
ember deploy hello-worker <version>
ember status hello-worker
```

### 9.3 管理一个已上线服务

```bash
ember logs hello-worker --limit 100
ember env set hello-worker APP_ENV production
ember rollback hello-worker <old-version>
```

## 10. 常见问题

### `ember build` 提示缺少 `wasm32-wasip2`

执行：

```bash
rustup target add wasm32-wasip2
```

### `ember publish` 提示找不到 artifact

先执行：

```bash
ember build
```

并检查 `worker.toml` 里的 `component` 路径是否正确。

### `ember dev` 启动了但访问不到接口

先确认：

- 监听地址是否正确
- `worker.toml` 中声明的 Wasm 产物是否已经构建成功
- 你的 handler 是否正确匹配请求路径
- 如果使用了 `base_path`，请求路径是否带上了对应前缀

## 11. 与源码对应

如果你需要确认 CLI 文档是否与实现一致，优先查看：

- `crates/ember-cli/src/main.rs`
- `crates/ember-cli/src/api.rs`
- `crates/ember-cli/src/config.rs`
