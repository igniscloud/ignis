# `sqlite-example`

一个最小的 Ignis SQLite project 示例：

- `api`：Rust `http` service，使用内置 SQLite host API 持久化一个计数器
- `web`：静态前端 service，请求同域 `/api` 并显示当前计数
- 路由模型：单个 project host，`web` 挂在 `/`，`api` 挂在 `/api`

## Project

这个 example 的 `ignis.hcl` 指向 project：

```hcl
project = {
  name = "sqlite-example"
}
```

## 本地运行

在这个目录下执行：

```bash
ignis service build --service api
ignis service build --service web
```

开两个终端：

```bash
ignis service dev --service api --skip-build --addr 127.0.0.1:3001
```

```bash
ignis service dev --service web --skip-build --addr 127.0.0.1:3000
```

然后访问：

```text
http://127.0.0.1:3000
```

预期行为：

- 页面默认请求 `http://127.0.0.1:3001` 并显示当前计数
- 点击 `Increment Counter` 会调用 `POST /increment`
- 刷新后仍能看到递增后的 SQLite 状态

## 部署后访问

发布到 igniscloud 兼容控制面后，这个示例的入口是：

- 前端：`https://<project-id>.<base-domain>/`
- API：`https://<project-id>.<base-domain>/api`
- 增加计数：`POST https://<project-id>.<base-domain>/api/increment`

当前模型是单个 project host 下按 path prefix 分流，不再需要单独的 API 子域名。
