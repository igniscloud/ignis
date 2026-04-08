# `ignis.toml` 配置文档

`ignis.toml` 是 Ignis 当前的项目级配置文件。它定义：

- project 名称
- project 下有哪些 service
- 每个 service 的本地相对路径
- 每个 service 的路径前缀路由
- `http` service 的 Wasm 运行配置
- `frontend` service 的静态构建配置

当前字段由 `ignis-manifest` 解析和校验，真实配置模型以 [ignis-manifest](../crates/ignis-manifest/src/lib.rs) 为准。

## 1. 最小示例

```toml
[project]
name = "hello-project"

[[services]]
name = "api"
kind = "http"
path = "services/api"
prefix = "/api"

[services.http]
component = "target/wasm32-wasip2/release/api.wasm"
base_path = "/"

[services.ignis_login]
display_name = "hello-project"
redirect_path = "/auth/common/callback"
providers = ["google"]
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
prefix = "/api"

[services.http]
component = "target/wasm32-wasip2/release/api.wasm"
base_path = "/api"

[services.ignis_login]
display_name = "pocket-tasks"
redirect_path = "/auth/common/callback"
providers = ["google"]

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

[[services]]
name = "web"
kind = "frontend"
path = "services/web"
prefix = "/"

[services.frontend]
build_command = ["npm", "run", "build"]
output_dir = "dist"
spa_fallback = true

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

#### `services[].prefix`

- 作用：service 在 project 域名下的路径前缀。
- 必填：是。
- 类型：`string`
- 约束：
  - 必须以 `/` 开头
  - project 内唯一
  - `/` 表示占用 project 根路径

### 3.3 `http` service 配置

`http` service 允许这些字段：

- `[services.http]`
- `[services.ignis_login]`
- `[services.env]`
- `[services.secrets]`
- `[services.sqlite]`
- `[services.resources]`
- `[services.network]`

#### `services.ignis_login`

- 作用：为当前 `http` service 声明一个由 Ignis control-plane 托管的 `common_server` confidential client。
- 必填：否。
- 说明：
  - 这是 service 级配置，不是 project 级配置
  - 第一版只支持 `confidential`
  - `client_id` / `client_secret` 由 control-plane 创建并写入当前 service 的 secrets
  - 当前 igniscloud hosted login 公网地址固定为 `https://cloud.transairobot.com`
  - 不要把 `COMMON_SERVER_BASE_URL` 作为 env 依赖

#### `services.ignis_login.display_name`

- 类型：`string`
- 作用：在 `common_server` 中创建 client 时使用的显示名
- 约束：
  - 不能为空

#### `services.ignis_login.redirect_path`

- 类型：`string`
- 作用：该 service 登录回调路径
- 约束：
  - 必须以 `/` 开头

#### `services.ignis_login.providers`

- 类型：`array<string>`
- 作用：要在 `common_server` 上打开的登录方式
- 约束：
  - 必须精确等于 `["google"]`

#### `ignis_login` 保留 secret 与不支持的 env

如果声明 `services.ignis_login`，当前 service 不允许手动声明这些名字：

- `IGNIS_LOGIN_CLIENT_ID`
- `IGNIS_LOGIN_CLIENT_SECRET`

另外，`COMMON_SERVER_BASE_URL` 也不应该作为 `services.env` 变量出现；当前 igniscloud 接入里请直接使用 `https://cloud.transairobot.com`。

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
  - 如果 service 声明了 `services.ignis_login`，建议显式包含 `cloud.transairobot.com`
  - 可用 `ignis service check --service <name>` 检查这类常见配置问题

### 3.4 `frontend` service 配置

`frontend` service 允许这些字段：

- `[services.frontend]`

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
- 不能定义 `[services.ignis_login]`
- 不能定义 `[services.env]`
- 不能定义 `[services.secrets]`
- 不能启用 sqlite
- 不能定义 `[services.resources]`
- 不能定义 `[services.network]`

### 3.5 `services[].prefix`

```toml
prefix = "/"
```

```toml
prefix = "/api"
```

- `/`
  - 绑定 `https://<project_id>.<base_domain>/`
- `/api`
  - 绑定 `https://<project_id>.<base_domain>/api`

规则：

- 同一个 project 内不能重复声明相同的 prefix
- ingress 按最长 prefix 匹配 service
- 匹配到非根 prefix 后，会先去掉 prefix 再把请求转给 service

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
- `services[].prefix`
  - 必须以 `/` 开头
  - project 内唯一
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
- 一个 project 域名下只保留一套 TLS 和 host，service 之间靠 path prefix 区分
- 需要访问外网时优先用 `allow_list`
- 敏感值不要写进 `[services.env]`，优先通过 `[services.secrets]` 绑定
- 根前缀 `/` 适合前端 service，API 等后台 service 建议显式挂到 `/api`、`/admin` 之类前缀

## 7. 相关文档

- [接入文档](./integration.md)
- [API 文档](./api.md)
- [CLI 文档](./cli.md)
