# `hello-fullstack`

一个最小的 Ignis 双 service 示例：

- `api`：Rust `http` service，提供 `GET /hello` handler，返回 `hello world`
- `web`：Vue 前端 service，请求后端并把返回内容显示在页面上
- 路由模型：单个 project host，`web` 挂在 `/`，`api` 挂在 `/api`

## Project

这个 example 的 `ignis.hcl` 指向 project：

```hcl
project = {
  name = "test"
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
ignis service dev --service api --addr 127.0.0.1:3001
```

```bash
ignis service dev --service web --skip-build --addr 127.0.0.1:3000
```

然后打开：

```text
http://127.0.0.1:3000
```

前端默认会请求：

```text
http://127.0.0.1:3001/hello
```

## 部署后访问

发布到 igniscloud 兼容控制面后，这个示例的入口是：

- 前端：`https://<project-id>.<base-domain>/`
- API：`https://<project-id>.<base-domain>/api/hello`

如果你给 project 绑定了自定义域名，那么 `ignis.hcl` 里的 `project.domain` 会切到这个自定义域名，访问入口也随之切换。

也就是说，当前模型是同一个 project host 下按 path prefix 分流，不再使用
`api.<project-host>` 这种 service 子域名。页面里的 `API Base` 默认也会推导成
同域的 `/api`。
