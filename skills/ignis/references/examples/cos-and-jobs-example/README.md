# `cos-and-jobs-example`

一个完整的 Google 登录 + 平台托管 COS 上传示例。

- `api`：Rust `http` service，负责 Google 登录回调、会话校验、每用户 10MB 配额、COS presign
- `web`：静态前端，登录后请求后端生成 presigned URL，并由浏览器直接 `PUT` 到 COS
- 配额：后端用 SQLite 按 Google user `sub` 记录 `pending` + `uploaded` 文件大小，每个用户最多 10MB
- 定时清理：每天北京时间 0 点运行 `cleanup_pending_uploads` job，删除超过 1 小时仍为 `pending` 的上传记录并释放额度
- COS 密钥：只在平台 control-plane / host 侧使用，不暴露给 Wasm 或浏览器

## 路由

- 前端：`/`
- 后端：`/api`
- 登录入口：`/api/auth/start`
- 登录回调：`/api/auth/callback`
- 当前用户：`GET /api/me`
- 上传列表：`GET /api/uploads`
- 创建上传：`POST /api/uploads/presign`
- 标记完成：`POST /api/uploads/complete`
- 下载签名：`GET /api/uploads/<file_id>/download`
- 清理 pending 上传 job handler：`POST /api/jobs/cleanup-pending-uploads`

## Job 和定时任务

`ignis.hcl` 声明了一个 `cleanup_pending_uploads` job，以及每天北京时间 0 点触发的 schedule：

- schedule：`midnight_cleanup_pending_uploads`
- cron：`0 0 * * *`
- timezone：`Asia/Shanghai`
- job target：`POST /api/jobs/cleanup-pending-uploads`

这个 handler 会删除 `status = 'pending'` 且 `created_at_ms` 早于当前时间 1 小时的上传记录。因为配额统计包含 `pending` + `uploaded`，删除这些过期 pending 记录后，对应用户的 10MB 额度会被释放。

Job handler 会检查 `x-ignis-job-*` headers，避免被普通前端流程误调用。生产环境如果要把 job handler 做成严格内部入口，建议使用单独未公开暴露的 job service 或平台级内部 dispatch。

## 构建

```bash
ignis service build --service api
ignis service build --service web
```

## 运行要求

control-plane 需要配置平台 `object_storage`，并且 COS bucket 需要允许浏览器对 presigned URL 发起跨域 `PUT`/`GET`。`ignis_login.providers = ["google"]` 会让平台为 `api` service 自动注入 `IGNIS_LOGIN_CLIENT_ID` 和 `IGNIS_LOGIN_CLIENT_SECRET`。

## 验证 10MB 限制

登录后上传多个文件。只要同一 Google 用户的 `pending` + `uploaded` 文件总大小超过 `10 * 1024 * 1024` bytes，`POST /api/uploads/presign` 会返回 `413`，前端会显示剩余额度不足。
