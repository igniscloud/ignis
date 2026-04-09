---
name: ignis-login
description: Use for Ignis login integration with services. Covers [services.ignis_login], IgnisCloud ID hosted login, google and test_password providers, PKCE, callback flows, and login smoke tests.
---

# Ignis Login

在当前任务是“给 Ignis service 接入登录，或排查登录链路”时使用这个 skill。

适用范围：

- 配置 `[services.ignis_login]`
- 设计或排查 `/auth/start`、`/auth/callback`、`/me`、`/logout`
- Hosted Login、PKCE、authorization code、userinfo
- `google` / `test_password` provider
- 登录 smoke test 和生产前去掉 `test_password`

不适用范围：

- 普通的 service 初始化、SQLite、非登录 API
  这些走 `ignis`
- 修改 `IgnisCloud ID` 或 `ignis-cli` 源码

## 快速流程

1. 先读 `references/igniscloud-id-public-api.md`。
2. 如果需要实际代码参考，读整个 `references/examples/ignis-login-example/` 项目。
3. 如果任务扩展到通用 service 集成、SQLite 或 `ignis.toml` 细节，切回 `ignis`。

## 工作规则

- `ignis_login` 只对声明了它的 `http` service 生效。
- 当前 provider 只支持 `google` 和 `test_password`。
- `providers` 采用 managed 模式：publish / deploy 时 control-plane 会把远端 `IgnisCloud ID` app 的 provider 集合同步到 manifest。
- `test_password` 只用于测试、联调和 smoke test；正式上线前应从 `providers` 中移除并重新发布部署。
- 默认测试账号密码是 `test / testtest`，除非服务端环境变量显式覆盖。
- 浏览器首入口统一走 Hosted `GET /login`，不要自行拼另一套第三方登录入口。
- 回调链路优先用 `authorization_code + PKCE + /oauth2/token + /oidc/userinfo`。

## 参考资料

- `IgnisCloud ID` Public API：`references/igniscloud-id-public-api.md`
- 登录 example 项目：`references/examples/ignis-login-example/`
