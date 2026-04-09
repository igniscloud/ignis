# IgnisCloud ID Public API 文档

本文描述 `IgnisCloud ID` 当前已经实现并对外可用的公共接口，面向前端、BFF、业务后端和客户端接入方。

文档约定：

- 地址是https://id.igniscloud.transairobot.com
- JSON 接口成功时统一返回 `{"data": ...}`
- JSON 接口失败时统一返回 `{"error": "错误信息"}`
- 时间字段使用 RFC3339 UTC 字符串
- `UUID` 字段使用标准 UUID 字符串
- `access_token` / `id_token` 为 JWT
- `refresh_token` 为服务端生成的长随机字符串

## 1. 通用规则

### 1.1 认证方式

- 用户接口：请求头 `authorization: Bearer <access_token>`
- `client_id`：业务 app 的客户端标识
- `client_secret`：`confidential` 类型 app 在 `/oauth2/token` 换码时使用
- `public` 类型 app 不允许传 `client_secret`
- 授权码模式下，当前接入规范要求 `confidential` 和 `public` 都携带 PKCE；区别只是在 token 端是否还需要 `client_secret`

### 1.2 通用错误码

| HTTP 状态码 | 含义 | 常见场景 |
| --- | --- | --- |
| `400 Bad Request` | 请求参数不合法 | 缺少字段、格式错误、PKCE 参数不正确 |
| `401 Unauthorized` | 鉴权失败 | token 无效、验证码错误、密码错误 |
| `404 Not Found` | 资源不存在 | app 不存在、资料不存在 |
| `409 Conflict` | 资源冲突 | `login_id` 已存在、验证码发送过于频繁 |
| `415 Unsupported Media Type` | 请求体类型不支持 | `/oauth2/token` 的 `Content-Type` 不支持 |
| `500 Internal Server Error` | 服务内部错误 | 数据库、Redis、签名等内部异常 |

### 1.3 常用枚举

#### `app_type`

| 值 | 含义 |
| --- | --- |
| `confidential` | 机密客户端，通常是服务端应用，换 token 时需要 `client_secret`，授权码模式也应带 PKCE |
| `public` | 公共客户端，通常是浏览器/移动端应用，必须使用 PKCE，换 token 时不能传 `client_secret` |

#### `provider`

| 值 | 含义 |
| --- | --- |
| `password` | 平台密码登录 |
| `google` | Google 登录 |
| `wechat` | 微信登录 |

#### `scene`

| 值 | 含义 |
| --- | --- |
| `register` | 注册验证码 |
| `login` | 登录验证码 |

## 2. 基础与 OIDC 元数据接口

### 2.1 健康检查

`GET /healthz`

响应：

```text
ok
```

### 2.2 OpenID Configuration

`GET /.well-known/openid-configuration`

说明：

- 返回当前 OIDC 元数据
- 可用于客户端发现授权端点、token 端点、JWKS 地址

响应字段：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `issuer` | string | 当前认证服务签发者标识，通常是服务公网地址 |
| `authorization_endpoint` | string | 标准 OAuth 授权端点地址 |
| `token_endpoint` | string | token 交换端点地址 |
| `userinfo_endpoint` | string | UserInfo 端点地址 |
| `jwks_uri` | string | 公钥集合地址 |
| `response_types_supported` | string[] | 当前支持的响应类型，当前固定为 `["code"]` |
| `subject_types_supported` | string[] | 当前支持的 subject 类型，当前固定为 `["public"]` |
| `id_token_signing_alg_values_supported` | string[] | 当前 ID Token 签名算法，当前固定为 `["EdDSA"]` |
| `grant_types_supported` | string[] | 当前支持的授权类型，当前为 `["authorization_code", "refresh_token"]` |
| `token_endpoint_auth_methods_supported` | string[] | token 端点支持的客户端鉴权方式，当前为 `["client_secret_post", "none"]` |
| `code_challenge_methods_supported` | string[] | PKCE 支持的方法，当前固定为 `["S256"]` |
| `scopes_supported` | string[] | 当前声明支持的 scope，当前为 `["openid", "profile"]` |
| `claims_supported` | string[] | `id_token` / `userinfo` 支持的 claims 列表 |
| `service_documentation` | string\|null | 文档地址，当前为空 |
| `extra.authorization_endpoints.password` | string | 密码授权端点 |
| `extra.authorization_endpoints.code` | string | 验证码授权端点 |
| `extra.authorization_endpoints.google` | string | Google 开发态直连授权端点 |
| `extra.authorization_endpoints.wechat` | string | 微信授权端点 |

说明：

- `extra.authorization_endpoints.google` 当前指向 `POST /oauth2/authorize/google`，用于开发态直传 `code`

### 2.3 JWKS 公钥集合

`GET /.well-known/jwks.json`

说明：

- 返回 JWT 验签公钥集合
- 当前签名算法是 `EdDSA`，曲线为 `Ed25519`

响应字段：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `keys` | object[] | 公钥列表 |
| `keys[].kty` | string | 密钥类型，当前固定为 `OKP` |
| `keys[].kid` | string | 当前公钥 ID，用于 JWT header 中的 `kid` 匹配 |
| `keys[].alg` | string | 签名算法，当前固定为 `EdDSA` |
| `keys[].use` | string | 用途，当前固定为 `sig` |
| `keys[].crv` | string | 曲线类型，当前固定为 `Ed25519` |
| `keys[].x` | string | Ed25519 公钥的 base64url 编码值 |

## 3. 账户与验证码接口

### 3.1 发送验证码

`POST /account/verification-codes/send`

说明：

- 同时用于注册验证码和登录验证码
- `target` 只能是邮箱或手机号
- 当前服务只生成并缓存验证码；是否真正发送短信/邮件由后续能力扩展决定
- 如果开启 `EXPOSE_DEBUG_VERIFICATION_CODE=true`，响应会返回 `debug_code`

请求体：

| 字段 | 类型 | 必填 | 含义 |
| --- | --- | --- | --- |
| `client_id` | string | 是 | app 客户端 ID |
| `target` | string | 是 | 验证码接收目标，必须是邮箱或手机号 |
| `scene` | string | 是 | 验证码场景，`register` 或 `login` |

响应字段：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `data.target` | string | 归一化后的目标值。邮箱会转为小写 |
| `data.scene` | string | 验证码场景 |
| `data.expires_in` | integer | 验证码有效期，单位秒 |
| `data.resend_after` | integer | 距离可再次发送还需等待的秒数 |
| `data.debug_code` | string\|null | 调试验证码，仅调试模式会返回 |

### 3.2 密码注册

`POST /account/register/password`

说明：

- 不允许直接使用任意 `login_id + password` 注册
- `login_id` 必须是邮箱或手机号
- 必须先发送 `register` 场景验证码，并在注册时提交 `verification_code`

请求体：

| 字段 | 类型 | 必填 | 含义 |
| --- | --- | --- | --- |
| `client_id` | string | 是 | app 客户端 ID |
| `login_id` | string | 是 | 登录标识，必须是邮箱或手机号 |
| `password` | string | 是 | 登录密码，最少 8 位 |
| `verification_code` | string | 是 | `register` 场景验证码 |
| `display_name` | string\|null | 否 | 初始展示名 |

成功响应：

- `201 Created`
- 无响应体

常见错误：

| 错误文案 | 含义 |
| --- | --- |
| `verification_code is required` | 未提供注册验证码 |
| `login_id/target must be a valid email or phone` | 登录标识不是合法邮箱或手机号 |
| `login_id already exists` | 该邮箱/手机号已经注册 |
| `verification code expired or missing` | 注册验证码不存在或已过期 |
| `invalid verification code` | 注册验证码不正确 |

### 3.3 浏览器托管登录入口

说明：

- 浏览器 Web 场景推荐优先走 `IgnisCloud ID` 自己托管的 `/login` 系列入口
- `GET /login` 会先校验 `client_id` / `redirect_uri`
- 如果浏览器已有 `cs_platform_session`，当前实现不会静默回跳，而是先展示“确认登录到目标 app”页面
- `POST /login/password` 和 `POST /login/continue` 成功后返回 `303 See Other`
- 使用 `303` 的目的是强制浏览器后续用 `GET` 打开业务回调地址，避免把原始表单 `POST` 转发到业务 app 导致 `405`

当前可用入口：

| 路径 | 方法 | 含义 |
| --- | --- | --- |
| `/login` | `GET` | 渲染 hosted login 页面或确认页 |
| `/login/password` | `POST` | 提交密码登录 |
| `/login/continue` | `POST` | 已有平台会话时确认继续登录目标 app |
| `/login/google` | `GET` | 拉起 Google 作为上游身份源 |
| `/login/wechat` | `GET` | 拉起微信作为上游身份源 |
| `/platform/logout` | `POST` | 清除 `IgnisCloud ID` 平台会话 cookie |

`GET /login`

请求参数：

| 字段 | 类型 | 必填 | 含义 |
| --- | --- | --- | --- |
| `client_id` | string | 是 | app 客户端 ID |
| `redirect_uri` | string | 是 | 目标 app 回调地址，必须已登记 |
| `state` | string\|null | 否 | 客户端透传状态 |
| `nonce` | string\|null | 否 | OIDC nonce |
| `code_challenge` | string\|null | 授权码模式必填 | PKCE challenge |
| `code_challenge_method` | string\|null | 与 `code_challenge` 成对出现 | 当前固定为 `S256` |
| `prompt` | string\|null | 否 | 可选 `login` / `select_account`，用于强制重新登录 |

说明：

- 当前接入规范要求：`confidential` 和 `public` 通过 `/login` 发起授权码登录时都带 PKCE
- `confidential` client 仍然需要在 `/oauth2/token` 一并提交 `client_secret + code_verifier`
- 如果缺少 PKCE，会收到与标准授权接口一致的错误；当前已知错误文案示例包括 `public clients must provide code_challenge`
- 如果 `code_challenge_method` 不是 `S256`，会收到 `only S256 code_challenge_method is supported`
- `prompt=login` 或 `prompt=select_account` 会强制忽略当前平台会话，重新展示登录 UI

## 4. OAuth/OIDC 授权接口

### 4.1 密码授权

`POST /oauth2/authorize/password`

说明：

- 使用邮箱/手机号 + 密码完成登录
- 成功后返回回跳地址，地址中带授权码 `code`
- 授权码模式应带 PKCE

请求体：

| 字段 | 类型 | 必填 | 含义 |
| --- | --- | --- | --- |
| `client_id` | string | 是 | app 客户端 ID |
| `redirect_uri` | string | 是 | 回调地址，必须已加入白名单 |
| `login_id` | string | 是 | 已注册的邮箱或手机号 |
| `password` | string | 是 | 登录密码 |
| `state` | string\|null | 否 | 客户端透传状态，防 CSRF |
| `nonce` | string\|null | 否 | OIDC nonce，用于 ID Token 防重放 |
| `code_challenge` | string\|null | 授权码模式必填 | PKCE challenge |
| `code_challenge_method` | string\|null | 与 `code_challenge` 成对出现 | 当前只支持 `S256` |

响应字段：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `data.redirect_to` | string | 回跳地址，包含授权码 `code`，以及原始 `state` |

### 4.2 验证码授权

`POST /oauth2/authorize/code`

说明：

- 使用邮箱/手机号 + 登录验证码登录
- 若该邮箱/手机号首次登录，会自动创建密码身份，但不会设置密码

请求体：

| 字段 | 类型 | 必填 | 含义 |
| --- | --- | --- | --- |
| `client_id` | string | 是 | app 客户端 ID |
| `redirect_uri` | string | 是 | 回调地址，必须已加入白名单 |
| `login_id` | string | 是 | 邮箱或手机号 |
| `verification_code` | string | 是 | `login` 场景验证码 |
| `display_name` | string\|null | 否 | 首次自动建用户时的展示名 |
| `state` | string\|null | 否 | 客户端透传状态 |
| `nonce` | string\|null | 否 | OIDC nonce |
| `code_challenge` | string\|null | 授权码模式必填 | PKCE challenge |
| `code_challenge_method` | string\|null | 与 `code_challenge` 成对出现 | 当前只支持 `S256` |

响应字段：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `data.redirect_to` | string | 回跳地址，包含授权码 `code` |

### 4.3 Google 授权

说明：

- 浏览器 Web 场景应统一走 hosted login：`GET /login`，然后由登录页里的 `GET /login/google` 拉起 Google
- `POST /oauth2/authorize/google` 仅保留为开发态直连流程
- Google 首次登录会自动创建用户，不需要手机号/邮箱验证码

补充：

- Google 回调到 `IgnisCloud ID` 的 provider callback 属于内部实现细节，不再作为业务接入入口对外文档化

#### 4.3.1 开发态直连流程

`POST /oauth2/authorize/google`

说明：

- 当前仓库里的 Google provider 仍保留开发态适配器
- `code` 可以不是 Google 官方 authorization code，而是本项目内部约定的调试字符串
- 当 `code` 为真实 Google authorization code 时，推荐优先走 `start/callback` 流程，由 `IgnisCloud ID` 自己完成跳转和回调处理

当前 `code` 格式：

```text
google_sub|email|display_name|avatar_url
```

字段说明：

| 位置 | 含义 |
| --- | --- |
| 第 1 段 | Google 用户唯一标识 `sub`，必填 |
| 第 2 段 | 邮箱，可为空 |
| 第 3 段 | 展示名，可为空 |
| 第 4 段 | 头像地址，可为空 |

请求体：

| 字段 | 类型 | 必填 | 含义 |
| --- | --- | --- | --- |
| `client_id` | string | 是 | app 客户端 ID |
| `redirect_uri` | string | 是 | 回调地址 |
| `code` | string | 是 | 调试态 Google code |
| `state` | string\|null | 否 | 客户端透传状态 |
| `nonce` | string\|null | 否 | OIDC nonce |
| `code_challenge` | string\|null | 授权码模式必填 | PKCE challenge |
| `code_challenge_method` | string\|null | 与 `code_challenge` 成对出现 | 当前只支持 `S256` |

响应字段：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `data.redirect_to` | string | 回跳地址，包含授权码 `code` |

### 4.4 微信授权

`POST /oauth2/authorize/wechat`

说明：

- 当前仓库里的微信 provider 是开发态适配器
- 微信首次登录会自动创建用户，不需要手机号/邮箱验证码

当前 `code` 格式：

```text
openid|unionid|display_name|avatar_url|provider_app_id
```

字段说明：

| 位置 | 含义 |
| --- | --- |
| 第 1 段 | 微信用户唯一标识 `openid`，必填 |
| 第 2 段 | `unionid`，可为空 |
| 第 3 段 | 展示名，可为空 |
| 第 4 段 | 头像地址，可为空 |
| 第 5 段 | 微信开放平台应用 ID，可为空 |

请求体与 Google 授权相同，只是 `code` 的解析规则不同。

响应字段：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `data.redirect_to` | string | 回跳地址，包含授权码 `code` |

### 4.5 换取 token

`POST /oauth2/token`

说明：

- 支持 `authorization_code` 和 `refresh_token`
- `confidential` app 使用 `client_secret`
- `public` app 不允许传 `client_secret`
- 授权码模式下，`confidential` 和 `public` 都应传 `code_verifier`
- 支持 `application/json` 和 `application/x-www-form-urlencoded`

请求体：

| 字段 | 类型 | 必填 | 含义 |
| --- | --- | --- | --- |
| `grant_type` | string | 是 | 授权类型，`authorization_code` 或 `refresh_token` |
| `client_id` | string | 是 | app 客户端 ID |
| `client_secret` | string\|null | `confidential` app 换码时必填 | app 客户端密钥 |
| `code` | string\|null | `authorization_code` 时必填 | 授权码 |
| `redirect_uri` | string\|null | `authorization_code` 时必填 | 与申请授权码时一致 |
| `code_verifier` | string\|null | `authorization_code` 时应提交 | PKCE verifier；`confidential` app 也应携带 |
| `refresh_token` | string\|null | `refresh_token` 时必填 | 刷新 token |

响应字段：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `data.access_token` | string | 访问 token，用于调用 UserInfo 等接口 |
| `data.refresh_token` | string | 刷新 token，刷新后会轮换 |
| `data.id_token` | string | OIDC ID Token，用于声明当前登录身份 |
| `data.token_type` | string | token 类型，当前固定为 `Bearer` |
| `data.expires_in` | integer | `access_token` 有效期，单位秒 |

### 4.6 退出当前登录

`POST /account/logout`

请求头：

```http
authorization: Bearer <access_token>
```

说明：

- 让当前 `access_token` 对应的 session 立即失效
- 适合业务后端或 BFF 在“退出登录”时调用
- 只会清理 `IgnisCloud ID` 自己维护的 session
- 不会清理 Google / 微信等外部 IdP 的浏览器登录态

成功响应：

- `204 No Content`

### 4.7 结束登录会话

`GET /oauth2/end-session`

说明：

- 用于 RP 发起的浏览器退出流程
- 根据 `id_token_hint` 中的 `sid` 让当前 session 失效
- 若传 `post_logout_redirect_uri`，必须是当前 app 已登记的 redirect URI
- 只会清理 `IgnisCloud ID` 自己维护的 session
- 不会清理 Google / 微信等外部 IdP 的浏览器登录态

请求参数：

| 字段 | 类型 | 必填 | 含义 |
| --- | --- | --- | --- |
| `id_token_hint` | string | 是 | 登录成功后签发的 OIDC `id_token` |
| `post_logout_redirect_uri` | string\|null | 否 | 退出后回跳地址，必须已登记 |
| `state` | string\|null | 否 | 回跳时原样附加到 redirect URI |
| `client_id` | string\|null | 否 | 额外的客户端一致性校验 |

成功响应：

- 未传 `post_logout_redirect_uri` 时返回 `204 No Content`
- 传了 `post_logout_redirect_uri` 时返回 `302 Found`

### 4.8 撤销 token

`POST /oauth2/revoke`

说明：

- 当前用于撤销 `refresh_token`
- 撤销后对应 session 会失效

请求体：

| 字段 | 类型 | 必填 | 含义 |
| --- | --- | --- | --- |
| `client_id` | string | 是 | app 客户端 ID |
| `client_secret` | string\|null | `confidential` app 必填 | app 客户端密钥 |
| `token` | string | 是 | 要撤销的 `refresh_token` |

成功响应：

- `204 No Content`

### 4.9 获取用户信息

`GET /oidc/userinfo`

请求头：

```http
authorization: Bearer <access_token>
```

说明：

- 返回当前 app 作用域下的用户信息
- `sub` 是 app 维度用户 ID，不同 app 下同一个平台用户可能不同

响应字段：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `data.sub` | string(UUID) | 当前 app 下的用户 ID |
| `data.aud` | string | 当前 token 面向的 `client_id` |
| `data.sid` | string(UUID) | 当前登录 session ID |
| `data.display_name` | string\|null | 展示名 |
| `data.avatar_url` | string\|null | 头像地址 |
| `data.locale` | string\|null | 语言地区偏好 |
| `data.timezone` | string\|null | 时区 |

## 5. 典型错误场景

### 5.1 授权码模式未传 PKCE

返回：

```json
{
  "error": "public clients must provide code_challenge"
}
```

说明：

- 这是当前已存在的一条错误示例
- 文档层面的接入规范已经调整为：`confidential` client 也应在授权码模式中携带 PKCE

### 5.2 注册时未传验证码

返回：

```json
{
  "error": "verification_code is required"
}
```

### 5.3 注册目标不是邮箱或手机号

返回：

```json
{
  "error": "login_id/target must be a valid email or phone"
}
```

### 5.4 验证码错误

返回：

```json
{
  "error": "invalid verification code"
}
```

### 5.5 refresh token 无效或已过期

返回：

```json
{
  "error": "refresh token expired"
}
```
