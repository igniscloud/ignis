# Ignis Service Link V1 Checklist

目标：把 `docs/ignis-service-link.md` 从“设计草案”推进到“可落地的 v1 内部链路”。

## 第一阶段

- [x] 固化 `CompiledProjectPlan` 的 `service_identity` / `binding_identity` / `protocol` / `public_exposures`
- [x] 在 runtime 中识别 `.svc` authority，并转发到 node-agent 内部 dispatch
- [x] 在 node-agent 中增加内部 HTTP dispatch 入口
- [x] 在 control-plane 暴露 project 级 binding registry API
- [x] 在 node-agent 中消费 binding registry，补齐内部 dispatch 的缺失状态和远端定位信息
- [x] runtime 对非法 `.svc` 目标返回明确错误，而不是回落成普通外发 HTTP

## 第二阶段

- [x] 让 `igniscloud` 部署链路开始消费编译后的 binding 计划
  当前形态：`ignis-cli publish` 会把 service 级 published plan 写入 `build_metadata`，control-plane deploy 和 binding registry 会优先消费这个元数据；`ignis-cli project sync` 也会阻断当前远端无法表达的多 binding / 非默认 binding service。
- [x] 用编译后的 binding 元数据驱动 node activation payload，而不是仅按 `ServiceKind` 推导单个默认 binding
- [x] 解除“每个 service 必须声明一个公网 exposure”的旧约束，并支持 internal-only service
- [x] 解除“同一 service 只能有一个公网 exposure”的旧约束
- [x] 让内部可调用性由 binding 决定，而不是由公网 route/prefix 兼容模型决定

## 第三阶段

- [x] 支持 remote-node 内部 HTTP 转发
- [x] 让 binding registry 稳定产出 active revision / active node / ingress URL
- [x] 为 `binding_not_found` / `binding_protocol_mismatch` / `no_active_revision_for_binding` 增加统一错误语义
  当前形态：node-agent internal dispatch 失败会返回稳定的 `x-ignis-error-code` 响应头，覆盖 `cross_project_service_access_denied` / `service_not_found_in_project` / `binding_not_found` / `binding_protocol_mismatch` / `no_active_revision_for_binding`。
- [x] 补充跨 node 集成测试

## 后续阶段

- [ ] 去掉当前 activation payload 中对 `route_prefix` 的兼容性依赖
