# Ignis Service Link

Status: v1 scope, HTTP only.

`Ignis Service Link`，简称 `ISL`，定义 Ignis 在同一个 project 内部的 HTTP 服务连接模型。它不依赖 Linux `ip:port`，而是基于 service identity、binding 和 control-plane / node-agent 的内部解析契约。

当前 v1 范围只覆盖：

- 同 project 内部服务访问
- `http` binding
- runtime 对 `.svc` authority 的内部解析
- control-plane binding registry
- node-agent 本地解析与跨 node HTTP 转发
- project 边界访问控制

当前明确不支持：

- 跨 project 服务访问
- streaming

## 1. 核心概念

### 1.1 Service

`service` 是部署单元，例如：

- `api`
- `users`
- `jobs`

### 1.2 Binding

`binding` 是 service 上的协议入口。当前 v1 只支持 `http` binding。

例子：

- `api#http`
- `jobs#http`

一个 service 可以声明多个 `http` binding，但只有被 exposure 引用的 binding 才会对公网暴露。

### 1.3 Exposure

`exposure` 只描述某个 binding 如何对公网暴露。

它不决定 project 内部是否可调用。

### 1.4 Listener

`listener` 是公网入口。当前只支持 `public/http` 这一类 HTTP listener。

## 2. Identity 规范

ISL 使用 service identity，而不是 `ip:port`。

标准格式：

- service identity
  - `svc://<project>/<service>`
- binding identity
  - `svc://<project>/<service>#<binding>`

例子：

- `svc://shop/api`
- `svc://shop/api#http`
- `svc://shop/jobs#http`

同 project 内允许简写：

- `api`
- `api#http`
- `jobs#http`

runtime 在解析时会自动补全当前 project。

## 3. 安全边界

v1 默认规则：

- 只允许访问当前 project 内的 service
- 跨 project 调用默认拒绝
- service 没有公网 exposure 时，仍然允许内部调用
- service 有公网 exposure 时，也不会因此获得跨 project 权限

也就是说：

- internal visibility 由 binding 决定
- public visibility 由 exposure 决定

resolver 必须在可信边界内做 project 校验，不能只靠 SDK 约定。

## 4. Transport

`http` binding 基于 `wasi:http`。

guest 继续发标准 HTTP 请求，但目标 authority 使用内部保留命名空间：

```text
http://api.svc/users/1
```

runtime 在出站 HTTP hook 中识别 `.svc`：

1. 不做真实 DNS 解析
2. 不走公网
3. 用 caller project 自动补全 target
4. 解析到 `svc://<current-project>/api#http`
5. 转发到本地或远端 node 上的活跃 revision

如果 authority 不是 `.svc`，则继续按普通外部 HTTP 处理。

## 5. Service Discovery

control-plane 维护 project 级 binding 路由表，最少包含：

- `project`
- `service`
- `binding`
- `protocol`
- active revision
- active node
- ingress URL
- public exposures

node-agent 缓存当前 node 的激活信息，并在本地未命中时回退到 control-plane binding registry。

## 6. Runtime 行为

HTTP 调用流程：

1. guest 发起 HTTP 请求
2. runtime 判断目标是内部 `.svc` 还是普通外部地址
3. 如果是内部：
   - 解析 service identity
   - 校验 `target_project == caller_project`
   - 查找 `http` binding 和 active revision
   - 本地直调或远端转发
4. 如果是外部：
   - 继续走当前外发 HTTP 路径

建议稳定错误码：

- `cross_project_service_access_denied`
- `service_not_found_in_project`
- `binding_not_found`
- `binding_protocol_mismatch`
- `no_active_revision_for_binding`

## 7. Activation Payload

node activation 需要携带：

- `service_identity`
- `binding_name`
- `protocol`
- `public_exposures`
- `worker_manifest`
- artifact metadata

这样 node-agent 才能同时处理：

- 公网 ingress
- project 内 service discovery

## 8. 编译计划

`ignis.hcl` 是源码，`CompiledProjectPlan` 是标准化后的部署与通信计划。

它至少稳定产出：

- service identities
- binding table
- exposure table
- activation plans

也就是说：

- `ignis.hcl` 定义用户意图
- `CompiledProjectPlan` 定义标准化后的通信和部署计划
- `igniscloud` / runtime / node-agent 消费该计划

## 9. 结论

ISL v1 当前就是 Ignis 的内部 HTTP 服务连接层。

它统一了：

- service identity
- http binding
- service discovery
- project 边界访问控制
- runtime / control-plane / node-agent 的解析契约
