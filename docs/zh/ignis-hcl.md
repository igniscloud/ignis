# `ignis.hcl` 配置文档

`ignis.hcl` 是 Ignis 当前唯一的项目级配置文件。它定义：

- `project` 名称
- `project` 当前线上访问域名
- 对外 listener
- listener 上的 exposure
- project 下有哪些 service
- 每个 service 的本地相对路径
- `http` service 的 Wasm 运行配置
- `frontend` service 的静态构建配置

当前字段由 `ignis-manifest` 中的 `ProjectSpec` 解析和校验，真实配置模型以 [ignis-manifest](../crates/ignis-manifest/src/project_hcl.rs) 为准。

## 1. 最小示例

```hcl
project = {
  name = "hello-project"
  domain = "prj-1234567890abcdef.transairobot.com"
}

listeners = [
  {
    name = "public"
    protocol = "http"
  }
]

exposes = [
  {
    name = "api"
    listener = "public"
    service = "api"
    path = "/"
  }
]

services = [
  {
    name = "api"
    kind = "http"
    path = "services/api"
    http = {
      component = "target/wasm32-wasip2/release/api.wasm"
      base_path = "/"
    }
    ignis_login = {
      display_name = "hello-project"
      redirect_path = "/auth/common/callback"
      providers = ["google"]
    }
  }
]
```

这个配置适合一个只有单个 `http` service 的最小链路。

## 2. 完整示例

```hcl
project = {
  name = "pocket-tasks"
  domain = "pockettasks.transairobot.com"
}

listeners = [
  {
    name = "public"
    protocol = "http"
  }
]

exposes = [
  {
    name = "api"
    listener = "public"
    service = "api"
    path = "/api"
  },
  {
    name = "web"
    listener = "public"
    service = "web"
    path = "/"
  }
]

services = [
  {
    name = "api"
    kind = "http"
    path = "services/api"
    bindings = [
      {
        name = "http"
        kind = "http"
      }
    ]
    http = {
      component = "target/wasm32-wasip2/release/api.wasm"
      base_path = "/api"
    }
    ignis_login = {
      display_name = "pocket-tasks"
      redirect_path = "/auth/common/callback"
      providers = ["google"]
    }
    env = {
      APP_NAME = "pocket-tasks"
      LOG_LEVEL = "info"
    }
    secrets = {
      OPENAI_API_KEY = "secret://openai-api-key"
    }
    sqlite = {
      enabled = true
    }
    resources = {
      memory_limit_bytes = 134217728
    }
  },
  {
    name = "web"
    kind = "frontend"
    path = "services/web"
    frontend = {
      build_command = ["npm", "run", "build"]
      output_dir = "dist"
      spa_fallback = true
    }
  }
]
```

## 3. 字段说明

### 3.1 `project`

#### `project.name`

- 作用：project 名称。
- 必填：是。
- 类型：`string`
- 约束：
  - 不能为空
  - 最长 48 个字符
  - 只允许字母、数字、`-`、`_`

#### `project.domain`

- 作用：记录当前 project 在线上的访问域名。
- 必填：否。
- 类型：`string`
- 约束：
  - 只允许写 host，不带 `https://`
  - 不能包含 path、query、fragment
  - 只允许字母、数字、`-`、`.`
- 当前行为：
  - `ignis project create` 会自动写入当前默认域名
  - `ignis project sync --mode apply` 如果发现本地缺少这个字段，会自动把线上当前域名写回 `ignis.hcl`
  - 如果本地 `project.domain` 和线上当前域名不一致，`ignis project sync` 会直接报错，要求先修正本地配置
  - `ignis domain create` / `ignis domain delete` 在当前目录就是该 project 时，也会同步更新这个字段

### 3.2 `listeners`

每个 listener 代表一个对外暴露入口。当前实现只支持 `http`。

#### `listeners[].name`

- 作用：listener 名称。
- 必填：是。
- 类型：`string`
- 约束：
  - project 内唯一
  - 不能为空
  - 最长 48 个字符
  - 只允许字母、数字、`-`、`_`

#### `listeners[].protocol`

- 作用：listener 协议。
- 必填：否。
- 默认值：`"http"`
- 当前可选值：
  - `http`

### 3.3 `exposes`

每个 `exposes[]` 把一个 service 绑定到某个 listener 上的公开路径。

#### `exposes[].name`

- 作用：exposure 名称。
- 必填：是。
- 约束：
  - project 内唯一

#### `exposes[].listener`

- 作用：引用一个 `listeners[].name`。
- 必填：是。
- 约束：
  - 必须引用已声明 listener

#### `exposes[].service`

- 作用：引用一个 `services[].name`。
- 必填：是。
- 约束：
  - 必须引用已声明 service

#### `exposes[].binding`

- 作用：指定要公开的 binding。
- 必填：否。
- 默认值：
  - `http` service 默认 `http`
  - `frontend` service 默认 `frontend`

#### `exposes[].path`

- 作用：公开路径前缀。
- 必填：否。
- 默认值：`"/"`
- 约束：
  - 必须以 `/` 开头
  - 同一 listener 下唯一

当前已经支持同一个 service 绑定多个公网 exposure，也支持不声明任何公网 exposure 的 internal-only service。

### 3.4 `services`

每个 `services[]` 代表一个发布和部署单元。

#### `services[].name`

- 作用：service 名称。
- 必填：是。
- 约束：
  - project 内唯一
  - 不能为空
  - 最长 48 个字符
  - 只允许字母、数字、`-`、`_`

#### `services[].kind`

- 作用：service 类型。
- 必填：是。
- 可选值：
  - `http`
  - `frontend`
  - `agent`

#### `services[].path`

- 作用：service 相对 project 根目录的路径。
- 必填：是。
- 约束：
  - 必须是相对路径
  - 不能是绝对路径
  - 不能包含 `..`

#### `services[].bindings`

- 作用：为 service 声明协议 binding。
- 必填：否。
- 默认值：
  - 空时会按 `kind` 合成默认 binding
- 当前约束：
  - `http` service 只允许 `http`
  - `frontend` service 只允许 `frontend`
  - `agent` service 只允许 `http`

### 3.5 `http` service 配置

`http` service 允许这些字段：

- `http`
- `ignis_login`
- `env`
- `secrets`
- `sqlite`
- `resources`

#### `services[].http.component`

- 作用：Wasm 组件文件路径。
- 必填：是。
- 说明：
  - 相对路径相对于该 service 目录
  - 常见值是 `target/wasm32-wasip2/release/<service>.wasm`

#### `services[].http.base_path`

- 作用：请求进入 guest 前的基础路径。
- 必填：否。
- 默认值：`"/"`
- 约束：
  - 必须以 `/` 开头

#### `services[].ignis_login`

- 作用：为当前 `http` service 声明一个由 control-plane 托管的 `IgnisCloud ID` confidential client。
- 必填：否。
- 说明：
  - 这是 service 级配置，不是 project 级配置
  - `client_id` / `client_secret` 由 control-plane 创建并写入当前 service 的 secrets
  - 当前 hosted login 公网地址固定为 `https://id.igniscloud.dev`
  - 不要把 `IGNISCLOUD_ID_BASE_URL` 作为 env 依赖

#### `services[].ignis_login.display_name`

- 类型：`string`
- 约束：
  - 不能为空

#### `services[].ignis_login.redirect_path`

- 类型：`string`
- 约束：
  - 必须以 `/` 开头

#### `services[].ignis_login.providers`

- 类型：`array<string>`
- 约束：
  - 不能为空
  - 当前只支持 `google` 和 `test_password`
  - 不能重复

#### `services[].env`

- 作用：普通环境变量。
- 类型：`object<string, string>`
- 默认值：空对象
- key 约束：
  - 只能使用 `A-Z`、`0-9`、`_`

#### `services[].secrets`

- 作用：secret 绑定。
- 类型：`object<string, string>`
- 默认值：空对象
- key 约束：
  - 只能使用 `A-Z`、`0-9`、`_`

#### `services[].sqlite.enabled`

- 作用：是否启用 SQLite host import。
- 类型：`bool`
- 默认值：`false`

#### `services[].resources.memory_limit_bytes`

- 类型：`integer`
- 约束：
  - 如果设置，必须大于 0

### 3.6 `frontend` service 配置

`frontend` service 允许这些字段：

- `frontend`

当前不允许为 `frontend` service 声明：

- `ignis_login`
- `env`
- `secrets`
- `sqlite`
- `resources`

#### `services[].frontend.build_command`

- 作用：构建静态站点的命令。
- 必填：是。
- 类型：`array<string>`
- 约束：
  - 不能为空

#### `services[].frontend.output_dir`

- 作用：构建输出目录。
- 必填：是。
- 类型：`string`

#### `services[].frontend.spa_fallback`

- 作用：是否启用 SPA fallback。
- 必填：否。
- 默认值：`false`

### 3.7 `agent` service 配置

`agent` service 用于托管运行 agent 框架的容器，例如 Codex 或 OpenCode。对用户暴露的是 `agent` service 语义；Podman 只是 node-agent 的底层执行实现。默认 runtime 是 Codex；如果要使用 OpenCode，设置 `agent_runtime = "opencode"`，并在 service 目录提供 `opencode.json`。

当产品需求需要 LLM 或 agent 能力时，优先使用内部 `agent` service 和 task API，而不是在业务 `http` service 里直接向模型 provider 发 HTTP 请求。这样 provider 凭据、runtime 启动、MCP tools、结果 schema 校验、callback 和轮询都留在平台托管的 agent 边界内。

Ignis 内置的 Codex 任务 agent 镜像为：

```hcl
{
  name = "agent-service"
  kind = "agent"
  path = "services/agent-service"
}
```

Ignis 会固定注入内置镜像、端口、工作目录、MCP URL、数据库路径、workspace 路径和 callback host allowlist，用户不需要配置这些字段。

内置 agent 暴露 `POST /v1/tasks`，每个任务启动一次 agent runtime，并存储通过 `task_result_json_schema` 校验的结果。如果任务提供 `callback_url`，结果会回调到该地址；否则调用方可以通过 `GET /v1/tasks/:task_id` 轮询结果。

OpenCode runtime 会启动 `opencode run`，部署时不需要 `OPENAI_API_KEY` secret；Ignis 会把 service 目录里的 `opencode.json` 注入到容器的 `$HOME/.config/opencode/opencode.json`。

创建 OpenCode agent service：

```bash
ignis service new \
  --service agent-service \
  --kind agent \
  --runtime opencode \
  --path services/agent-service
```

对应的 `ignis.hcl`：

```hcl
{
  name = "agent-service"
  kind = "agent"
  agent_runtime = "opencode"
  path = "services/agent-service"
}
```

发布前在 service 目录提供 OpenCode 运行配置：

```bash
cp ~/.config/opencode/opencode.json services/agent-service/opencode.json
chmod 600 services/agent-service/opencode.json
```

`opencode.json` 可能包含 provider 凭据，应放在版本控制之外，并避免打印到日志。发布时 Ignis 会把它作为 agent artifact 上传；部署时 node-agent 会只读挂载到：

```text
/agent-home/.config/opencode/opencode.json
```

同一个 project 内的其他 service 通过内部 service DNS 调用：

```text
POST http://agent-service.svc/v1/tasks
GET  http://agent-service.svc/v1/tasks/{task_id}
```

创建 task 的请求体：

```json
{
  "prompt": "...",
  "callback_url": "可选的 http 或 https URL",
  "task_result_json_schema": {
    "type": "object"
  }
}
```

`task_result_json_schema` 是 agent 最终通过 `submit_task` 提交的 `result` 的 JSON Schema。如果不传 `callback_url`，调用方通过 `GET /v1/tasks/{task_id}` 轮询，直到 `status` 为 `succeeded` 或 `failed`。

当前版本不支持用户自定义 agent 镜像。

`agent` service 对用户暴露的主要字段：

- `name`
- `kind`
- `path`
- `agent_runtime`
- `resources.memory_limit_bytes`

当前不允许为 `agent` service 声明：

- `http`
- `frontend`
- `ignis_login`
- `sqlite`
- `agent`
- `env`
- `secrets`

## 4. 当前未实现的 HCL 领域

下面这些概念已经在架构设计和 TODO 里出现，但当前代码还没有正式落地：

- `dependencies`
- `imported_services`
- `config`
- `mounts`
- `package`
- `lockfile`

当前公开可用的 HCL 范围，仍然聚焦在 `http` / `frontend` / `agent` project 配置。
