# `ignis.toml` 配置文档

`ignis.toml` 是 Ignis 当前的项目级配置文件。它定义：

- project 名称
- project 下有哪些 service
- 每个 service 的本地相对路径
- `http` service 的 Wasm 运行配置
- `frontend` service 的静态构建配置
- service 的域名绑定

当前字段由 `ignis-manifest` 解析和校验，真实配置模型以 [ignis-manifest](../crates/ignis-manifest/src/lib.rs) 为准。

## 1. 最小示例

```toml
[project]
name = "hello-project"

[[services]]
name = "api"
kind = "http"
path = "services/api"

[services.http]
component = "target/wasm32-wasip2/release/api.wasm"
base_path = "/"
```

这个配置适合一个只有单个 `http` service 的最小本地链路。

## 2. 完整示例

```toml
[project]
name = "pocket-tasks"

[[services]]
name = "api"
kind = "http"
path = "services/api"

[services.http]
component = "target/wasm32-wasip2/release/api.wasm"
base_path = "/api"

[services.env]
APP_NAME = "pocket-tasks"
LOG_LEVEL = "info"

[services.secrets]
OPENAI_API_KEY = "secret://openai-api-key"

[services.sqlite]
enabled = true

[services.resources]
cpu_time_limit_ms = 5000
memory_limit_bytes = 134217728

[services.network]
mode = "allow_list"
allow = ["api.openai.com:443", ".example.com"]

[[services.bindings]]
host = "api"

[[services]]
name = "web"
kind = "frontend"
path = "services/web"

[services.frontend]
build_command = ["npm", "run", "build"]
output_dir = "dist"
spa_fallback = true

[[services.bindings]]
host = "@"

[[services.bindings]]
host = "www"
```

## 3. 字段说明

### 3.1 `[project]`

#### `project.name`

- 作用：project 名称。
- 必填：是。
- 类型：`string`
- 约束：
  - 不能为空
  - 最长 48 个字符
  - 只允许字母、数字、`-`、`_`

### 3.2 `[[services]]`

每个 `[[services]]` 代表一个发布和部署单元。

#### `services[].name`

- 作用：service 名称。
- 必填：是。
- 类型：`string`
- 约束：
  - 在 project 内唯一
  - 不能为空
  - 最长 48 个字符
  - 只允许字母、数字、`-`、`_`

#### `services[].kind`

- 作用：service 类型。
- 必填：是。
- 可选值：
  - `http`
  - `frontend`

#### `services[].path`

- 作用：service 相对 project 根目录的路径。
- 必填：是。
- 类型：`string`
- 约束：
  - 必须是相对路径
  - 不能是绝对路径
  - 不能包含 `..`

### 3.3 `http` service 配置

`http` service 允许这些字段：

- `[services.http]`
- `[services.env]`
- `[services.secrets]`
- `[services.sqlite]`
- `[services.resources]`
- `[services.network]`
- `[[services.bindings]]`

#### `services.http.component`

- 作用：Wasm 组件文件路径。
- 必填：是。
- 类型：`string`
- 说明：
  - 相对路径相对于该 service 目录
  - 常见值是 `target/wasm32-wasip2/release/<service>.wasm`

#### `services.http.base_path`

- 作用：请求进入 guest 前的基础路径。
- 必填：否。
- 类型：`string`
- 默认值：`"/"`
- 约束：
  - 必须以 `/` 开头

#### `services.env`

- 作用：普通环境变量。
- 类型：`table<string, string>`
- 默认值：空表
- key 约束：
  - 只能使用 `A-Z`、`0-9`、`_`

#### `services.secrets`

- 作用：secret 绑定。
- 类型：`table<string, string>`
- 默认值：空表
- key 约束：
  - 只能使用 `A-Z`、`0-9`、`_`

#### `services.sqlite.enabled`

- 作用：是否启用 SQLite host import。
- 类型：`bool`
- 默认值：`false`

#### `services.resources.cpu_time_limit_ms`

- 类型：`integer`
- 约束：
  - 如果设置，必须大于 0

#### `services.resources.memory_limit_bytes`

- 类型：`integer`
- 约束：
  - 如果设置，必须大于 0

#### `services.network.mode`

- 可选值：
  - `deny_all`
  - `allow_list`
  - `allow_all`

#### `services.network.allow`

- 类型：`array<string>`
- 说明：
  - 仅当 `mode = "allow_list"` 时允许设置
  - 当 `mode = "allow_list"` 时不能为空
  - 支持 `host`、`host:port`、`.suffix`、`[ipv6]:port`

### 3.4 `frontend` service 配置

`frontend` service 允许这些字段：

- `[services.frontend]`
- `[[services.bindings]]`

#### `services.frontend.build_command`

- 作用：构建静态站点时执行的命令。
- 必填：是。
- 类型：`array<string>`
- 约束：
  - 不能为空

#### `services.frontend.output_dir`

- 作用：构建输出目录。
- 必填：是。
- 类型：`string`

#### `services.frontend.spa_fallback`

- 作用：静态托管时是否回退到 `index.html`。
- 必填：否。
- 类型：`bool`
- 默认值：`false`

#### `frontend` service 限制

- 不能定义 `[services.http]`
- 不能定义 `[services.env]`
- 不能定义 `[services.secrets]`
- 不能启用 sqlite
- 不能定义 `[services.resources]`
- 不能定义 `[services.network]`

### 3.5 `[[services.bindings]]`

```toml
[[services.bindings]]
host = "@"
```

```toml
[[services.bindings]]
host = "api"
```

```toml
[[services.bindings]]
host = "example.com"
```

- `@`
  - 绑定 `<project_id>.<base_domain>`
- `api`
  - 绑定 `api.<project_id>.<base_domain>`
- `example.com`
  - 绑定完整自定义域名

规则：

- 同一个 project 内不能重复声明相同的 binding
- `@` 不会自动绑定给任何 service
- project 没有默认 service

## 4. 默认值汇总

```toml
[services.sqlite]
enabled = false

[services.resources]

[services.network]
mode = "deny_all"
allow = []
```

`http.base_path` 默认是 `"/"`，`frontend.spa_fallback` 默认是 `false`。

## 5. 校验规则汇总

- `project.name` 和 `services[].name`
  - 不能为空
  - 最长 48 个字符
  - 只允许字母、数字、`-`、`_`
- `services[].path`
  - 必须是相对路径
  - 不能包含 `..`
- `http.base_path`
  - 必须以 `/` 开头
- `http.component`
  - 不能为空
- `[services.env]` / `[services.secrets]` 的 key
  - 只能使用 `A-Z`、`0-9`、`_`
- `resources.cpu_time_limit_ms`
  - 如果设置，必须大于 0
- `resources.memory_limit_bytes`
  - 如果设置，必须大于 0
- `network.allow`
  - 只有 `network.mode = "allow_list"` 时才允许设置
  - `allow_list` 模式下不能为空
- `frontend.build_command`
  - 不能为空
- `frontend.output_dir`
  - 不能为空

## 6. 使用建议

- 让一个 project 承载一组相关 service，不要再把单个 service 当作完整 project
- `http` service 和 `frontend` service 分目录管理，例如 `services/api`、`services/web`
- 需要访问外网时优先用 `allow_list`
- 敏感值不要写进 `[services.env]`，优先通过 `[services.secrets]` 绑定
- 没有任何默认 service，域名访问完全由 `bindings` 决定

## 7. 相关文档

- [接入文档](./integration.md)
- [API 文档](./api.md)
- [CLI 文档](./cli.md)
