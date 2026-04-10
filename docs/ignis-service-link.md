# Ignis Service Link

Status: draft for the first internal service communication model in Ignis.

`Ignis Service Link`，简称 `ISL`，定义 Ignis 在 project 内部的服务连接层。它不把服务通信建立在 Linux `ip:port` 上，而是建立在 service identity、binding 和 protocol 之上。

ISL 负责的不是单一“通信协议”，而是一整套内部连接语义：

- service identity
- binding / protocol 声明
- 同 project 内的服务发现
- runtime 内部路由与转发
- control-plane / node-agent 的解析契约
- project 边界下的访问控制

## 1. 目标

v1 目标：

- 只支持同 project 内部服务访问
- 支持多个 binding
- 至少支持两类协议：
  - `http`
  - `rpc`
- 将公网 exposure 和内部调用解耦
- 不要求 `igniscloud` 直接解析 HCL 文本
- 只要求 `igniscloud` 理解编译后的服务计划

非目标：

- 跨 project 服务访问
- 平台托管式 gateway DSL
- 首版直接实现 `grpc`
- 流式协议和复杂多路复用

## 2. 核心概念

### 2.1 Service

`service` 是部署单元，例如：

- `api`
- `users`
- `jobs`

### 2.2 Binding

`binding` 是 service 上的一个协议入口。协议能力挂在 binding 上，而不是挂在 service 上。

例子：

- `api#http`
- `jobs#rpc`

一个 service 可以有多个 binding，但首版实现可以从“每个 service 一个主 binding”开始。

### 2.3 Exposure

`exposure` 只表示某个 binding 如何对公网暴露。

它不决定 project 内部是否可调用。

### 2.4 Listener

`listener` 是公网入口，例如 `public/http`。它属于外部流量入口，不属于内部服务发现本身。

## 3. Identity 规范

ISL 使用 service identity 而不是 `ip:port`。

标准格式：

- service identity
  - `svc://<project>/<service>`
- binding identity
  - `svc://<project>/<service>#<binding>`

例子：

- `svc://shop/api`
- `svc://shop/api#http`
- `svc://shop/jobs#rpc`

同 project 内允许简写：

- `api`
- `api#http`
- `jobs#rpc`

runtime 在解析时自动补全当前 project。

## 4. 安全边界

v1 的默认规则必须是：

- 只允许访问当前 project 内的 service
- 跨 project 调用默认拒绝
- 不因为 service 没有公网 exposure 就禁止内部调用
- 不因为 service 有公网 exposure 就允许跨 project 调用

换句话说：

- internal visibility 由 `binding` 决定
- public visibility 由 `exposure` 决定

这两者必须分开。

resolver 必须在可信边界里强制做 project 校验，而不是只靠 SDK 或文档约定。

## 5. 协议模型

binding 上声明 protocol。

示例：

```hcl
services = [
  {
    name = "api"
    kind = "http"
    path = "services/api"
    bindings = [
      { name = "http", kind = "http" }
    ]
  },
  {
    name = "jobs"
    kind = "http"
    path = "services/jobs"
    bindings = [
      { name = "rpc", kind = "rpc" }
    ]
  }
]
```

含义：

- `api#http` 接受 HTTP
- `jobs#rpc` 接受 request/response 风格的 RPC

首版建议只支持：

- `http`
- `rpc`

`grpc` 可以保留为未来 binding kind，但不纳入首版交付。

## 6. Transport 策略

### 6.1 HTTP

`http` binding 基于 `wasi:http`。

guest 继续用标准 HTTP client 发请求，但目标 authority 使用内部保留命名空间。

例如：

```text
http://api.shop.svc/users/1
```

这里：

- `scheme = http`
- `authority = api.shop.svc`
- `path = /users/1`

runtime 在出站 HTTP hook 中识别 `.svc` 命名空间：

1. 不做真实 DNS 解析
2. 不走公网
3. 直接把请求解析为内部 target
4. 解析到 `svc://shop/api#http`
5. 转发到本地或远端 node 上的活跃 revision

### 6.2 外部 HTTP

如果 authority 不在内部保留命名空间里，例如：

- `api.openai.com`
- `example.com`

则按普通外部 HTTP 处理：

- 继续走现有 `wasi:http` 外发路径
- 继续受 `network.mode` / `network.allow` 控制

规则必须是：

- 内部 service 调用不走外部 allowlist
- 外部 HTTP 继续走 allowlist

### 6.3 RPC

`rpc` binding 不建议伪装成 URL。

建议走专门的 host ABI，例如：

```text
service.call("svc://shop/jobs#rpc", "enqueue", headers, body)
```

这样可以避免把非 HTTP 协议强行塞进 URL / method / path 模型中。

## 7. Service Discovery

control-plane 需要维护一张 binding 路由表。最少包含：

- `project`
- `service`
- `binding`
- `protocol`
- active revision
- assigned nodes
- public exposures

node-agent 需要缓存当前 project 范围内的解析信息。

resolver 输入：

- caller project
- caller service
- target identity

resolver 输出：

- target project
- target service
- target binding
- protocol
- active revision
- local node or remote node decision

## 8. Runtime 行为

runtime 调用流程：

1. guest 发起请求
2. runtime 判断目标是内部还是外部
3. 如果是内部：
   - 解析 service identity
   - 校验 `target_project == caller_project`
   - 查找 binding 和 active revision
   - 本地直调或远端转发
4. 如果是外部：
   - 继续走当前外发 HTTP 路径
   - 继续做 outbound allowlist 校验

建议错误码至少有：

- `cross_project_service_access_denied`
- `service_not_found_in_project`
- `binding_not_found`
- `binding_protocol_mismatch`
- `no_active_revision_for_binding`

## 9. Activation Payload 演进

当前 node activation 主要围绕：

- `route_prefix`
- `WorkerManifest`

未来建议升级为：

- `service_identity`
- `binding_name`
- `protocol`
- `public_exposures`
- `worker_manifest`
- artifact metadata

这样 node-agent 才能同时处理：

- 公网 ingress
- project 内 service discovery

## 10. HCL 与编译计划

`ignis.hcl` 是源码。

`CompiledProjectPlan` 应当成为 ISL 的标准输入计划，而不是仅仅作为兼容旧 `prefix` 模型的中间产物。

它至少需要稳定产出：

- service identities
- binding table
- exposure table
- activation plans

也就是说：

- `ignis.hcl` 定义用户意图
- `CompiledProjectPlan` 定义标准化后的通信和部署计划
- `igniscloud` / runtime / node-agent 消费该计划

`igniscloud` 不一定需要解析原始 HCL 文本，但最终必须理解这份编译后的计划语义。

## 11. 首版范围

建议首版只交付：

- 同 project service identity
- `http` binding
- `rpc` binding 结构定义
- 基于保留 authority 的内部 HTTP 解析
- control-plane binding registry
- node-agent 本地 resolver
- runtime 内部 HTTP 转发

先不交付：

- cross-project access
- `grpc`
- streaming
- service-to-service auth policy
- retry / timeout / rate-limit DSL
- package/imported service 联动

## 12. 实现顺序

建议按下面顺序推进：

1. 固化 `CompiledProjectPlan` 的 identity / binding / exposure 输出
2. 在 `igniscloud` 增加 binding registry
3. 升级 node activation payload
4. 在 `ignis-runtime` 中实现内部 HTTP resolver
5. 在 `ignis-host-abi` 中增加 `rpc` service call ABI

## 13. 结论

ISL 是 Ignis 的内部服务连接层。

它不是单一传输协议，而是把以下内容统一起来：

- service identity
- binding / protocol
- service discovery
- project 边界访问控制
- runtime / control-plane / node-agent 之间的解析契约

对于 Ignis 来说，最合理的演进路径是：

- `http` 先用 `wasi:http + internal authority`
- `rpc` 再用 host ABI
- `grpc` 留到后续阶段
