# `worker.toml` 配置文档

`worker.toml` 是 Ignis worker 的描述文件。它定义了 worker 名称、Wasm 构件路径、请求基础路径，以及运行时需要的环境变量、secret、SQLite、资源限制、出站网络策略和 igniscloud 发布配置。

当前字段由 `ignis-manifest` 解析和校验，真实配置模型以 [ignis-manifest](../crates/ignis-manifest/src/lib.rs) 为准。

## 1. 最小示例

```toml
name = "hello-worker"
component = "target/wasm32-wasip2/release/hello_worker.wasm"
base_path = "/"
```

这个配置适合本地 `ignis build` 和 `ignis dev` 的最小链路。

## 2. 完整示例

```toml
name = "pocket-tasks"
component = "target/wasm32-wasip2/release/pocket_tasks_worker.wasm"
base_path = "/api"

[env]
APP_NAME = "pocket-tasks"
LOG_LEVEL = "info"

[secrets]
OPENAI_API_KEY = "secret://openai-api-key"

[sqlite]
enabled = true

[resources]
cpu_time_limit_ms = 5000
memory_limit_bytes = 134217728

[network]
mode = "allow_list"
allow = ["api.openai.com:443", ".example.com"]

[igniscloud]
app = "pocket-tasks"
```

## 3. 字段说明

### 3.1 顶层字段

#### `name`

- 作用：worker 名称。
- 必填：是。
- 类型：`string`。
- 约束：
  - 不能为空
  - 最长 48 个字符
  - 只允许字母、数字、`-`、`_`
- 说明：
  - 这是 manifest 自身的名称，不一定等同于云端 app，但通常建议保持一致。

#### `component`

- 作用：Wasm 组件文件路径。
- 必填：是。
- 类型：`string`
- 约束：
  - 不能为空
- 说明：
  - 一般写构建产物的相对路径，例如 `target/wasm32-wasip2/release/hello_worker.wasm`
  - 解析时相对路径是相对于 `worker.toml` 所在目录，而不是当前 shell 工作目录

#### `base_path`

- 作用：请求进入 guest 前使用的基础路径。
- 必填：否。
- 类型：`string`
- 默认值：`"/"`
- 约束：
  - 必须以 `/` 开头
- 说明：
  - 适合把 worker 挂在某个前缀下，例如 `/api`
  - 如果设置为 `/api`，运行时会先按这个前缀匹配和重写请求路径

### 3.2 `[env]`

```toml
[env]
APP_NAME = "hello-worker"
LOG_LEVEL = "debug"
```

- 作用：向 worker 注入普通环境变量。
- 必填：否。
- 类型：`table<string, string>`
- 默认值：空表
- key 约束：
  - 只能使用 `A-Z`、`0-9`、`_`
- 说明：
  - 适合放非敏感配置
  - 这些值会直接暴露给 guest 运行时

### 3.3 `[secrets]`

```toml
[secrets]
OPENAI_API_KEY = "secret://openai-api-key"
```

- 作用：定义 secret 绑定。
- 必填：否。
- 类型：`table<string, string>`
- 默认值：空表
- key 约束：
  - 只能使用 `A-Z`、`0-9`、`_`
- 说明：
  - 在 igniscloud 或兼容控制面里，通常写成 `secret://<secret-name>`
  - manifest 自身只校验 key，不强制 value 格式；具体如何解析由外部平台决定
  - 常见用法是把平台 secret 映射成 worker 里的环境变量名

### 3.4 `[sqlite]`

```toml
[sqlite]
enabled = true
```

- 作用：声明是否启用 SQLite host import。
- 必填：否。
- 类型：table
- 默认值：

```toml
[sqlite]
enabled = false
```

- 说明：
  - 设为 `true` 后，worker 可以通过 `ignis_sdk::sqlite` 访问默认数据库
  - 设为 `false` 时，不应该假设运行时会提供 SQLite 能力

### 3.5 `[resources]`

```toml
[resources]
cpu_time_limit_ms = 5000
memory_limit_bytes = 134217728
```

- 作用：声明运行时资源限制。
- 必填：否。
- 类型：table

#### `resources.cpu_time_limit_ms`

- 作用：CPU 时间限制，单位毫秒。
- 类型：`integer`
- 默认值：未设置
- 约束：
  - 如果设置，必须大于 0
- 说明：
  - 由运行时通过 epoch interruption 等机制执行限制

#### `resources.memory_limit_bytes`

- 作用：内存限制，单位字节。
- 类型：`integer`
- 默认值：未设置
- 约束：
  - 如果设置，必须大于 0
- 说明：
  - 常见值例如 `134217728` 表示 128 MiB

### 3.6 `[network]`

```toml
[network]
mode = "allow_list"
allow = ["api.openai.com:443", ".example.com"]
```

- 作用：声明 worker 的出站网络策略。
- 必填：否。
- 类型：table
- 默认值：

```toml
[network]
mode = "deny_all"
allow = []
```

#### `network.mode`

- 可选值：
  - `deny_all`
  - `allow_list`
  - `allow_all`

#### `network.allow`

- 类型：`array<string>`
- 说明：
  - 仅当 `mode = "allow_list"` 时允许设置
  - 当 `mode = "allow_list"` 时不能为空
  - 支持这些格式：
    - `host`
    - `host:port`
    - `.suffix`
    - `[ipv6]:port`

#### 行为说明

- `deny_all`
  - 拒绝所有出站网络访问
  - `allow` 必须为空
- `allow_all`
  - 允许所有出站网络访问
  - `allow` 必须为空
- `allow_list`
  - 只允许命中白名单规则的目标
  - `.example.com` 会匹配 `example.com` 和其子域名

### 3.7 `[igniscloud]`

```toml
[igniscloud]
app = "hello-worker"
```

- 作用：声明当前 worker 对应的云端 app。
- 必填：否。
- 类型：table

#### `igniscloud.app`

- 作用：指定发布和部署时使用的 igniscloud app 名称。
- 类型：`string`
- 默认值：未设置
- 约束：
  - 如果设置，不能为空白
  - 最长 48 个字符
  - 只允许字母、数字、`-`、`_`
- 说明：
  - `ignis app publish` / `ignis app deploy` 会优先使用这里的 app
  - 如果没有填写，CLI 通常需要你在命令行显式指定 app，或者直接报错提示补充配置

## 4. 默认值汇总

```toml
base_path = "/"

[env]

[secrets]

[sqlite]
enabled = false

[resources]

[network]
mode = "deny_all"
allow = []
```

`[igniscloud]` 默认不写入；只有配置了 `app` 时才会出现在渲染结果里。

## 5. 校验规则汇总

- `name` 和 `igniscloud.app`
  - 不能为空
  - 最长 48 个字符
  - 只允许字母、数字、`-`、`_`
- `base_path`
  - 必须以 `/` 开头
- `component`
  - 不能为空
- `[env]` / `[secrets]` 的 key
  - 只能使用 `A-Z`、`0-9`、`_`
- `resources.cpu_time_limit_ms`
  - 如果设置，必须大于 0
- `resources.memory_limit_bytes`
  - 如果设置，必须大于 0
- `network.allow`
  - 只有 `network.mode = "allow_list"` 时才允许设置
  - `allow_list` 模式下不能为空

## 6. 常见配置模板

### 6.1 最小 HTTP worker

```toml
name = "hello-worker"
component = "target/wasm32-wasip2/release/hello_worker.wasm"
base_path = "/"
```

### 6.2 带 SQLite 的 worker

```toml
name = "sqlite-worker"
component = "target/wasm32-wasip2/release/sqlite_worker.wasm"
base_path = "/"

[sqlite]
enabled = true
```

### 6.3 带 secret 和 igniscloud 发布配置的 worker

```toml
name = "secret-worker"
component = "target/wasm32-wasip2/release/secret_worker.wasm"
base_path = "/"

[secrets]
GREETING = "secret://greeting"

[igniscloud]
app = "secret-worker"
```

## 7. 使用建议

- 本地开发先保留最小配置，只加当前需要的能力
- 敏感值不要写进 `[env]`，优先通过 `[secrets]` 绑定
- 如果 worker 需要访问外网，优先使用 `allow_list`，不要默认 `allow_all`
- 如果准备发布到 igniscloud，建议始终填写 `[igniscloud].app`
- 如果修改了 `component` 路径，确认它仍然指向实际构建产物

## 8. 相关文档

- [接入文档](./integration.md)
- [API 文档](./api.md)
- [CLI 文档](./cli.md)
