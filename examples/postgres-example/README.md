# `postgres-example`

一个最小的 Ignis Postgres + MySQL project 示例，用来验证 CN region 上
数据库 host import 的端到端链路：

- `api`：Rust `http` service，使用 `ignis_sdk::postgres`
- `api`：同一个 service 也使用 `ignis_sdk::mysql`
- `web`：静态前端 service，请求同域 `/api`
- 路由模型：单个 project host，`web` 挂在 `/`，`api` 挂在 `/api`
- Postgres：平台托管 `postgres.enabled = true`
- MySQL：通过 `IGNIS_MYSQL_URL` secret 注入外部 MySQL URL
- 数据库动作：建表、seed、查询、事务更新、批量事务、参数绑定、类型读取
- MySQL host 使用全局 `sqlx::MySqlPool`，默认 `max_connections = 64`

## Project

这个 example 已经通过 CN region 创建并绑定远端 project：

```hcl
project = {
  name = "postgres-example"
  domain = "prj-87e446f7f183c261.transairobot.com"
}
```

本地状态在 `.ignis/project.json`，其中 `region` 是 `cn`。后续
`publish` / `deploy` 会使用这个 project-local region，而不是当前默认账号。

## 本地构建

在这个目录下执行：

```bash
ignis service check --service api
ignis service build --service api
ignis service build --service web
```

## MySQL secret

工作区根目录的 `test.txt` 使用下面格式保存 MySQL 连接信息：

```text
url: <mysql-host>
user: <mysql-user>
password: <mysql-password>
```

部署前把它转换为 `IGNIS_MYSQL_URL` 并写入 `mysql-url` secret。不要把真实密码写进
`ignis.hcl` 或 README。

```bash
TEST_TXT="${TEST_TXT:-../../../test.txt}"
MYSQL_HOST="$(awk -F': ' '/^url:/ {print $2}' "$TEST_TXT")"
MYSQL_USER="$(awk -F': ' '/^user:/ {print $2}' "$TEST_TXT")"
MYSQL_PASSWORD="$(awk -F': ' '/^password:/ {print $2}' "$TEST_TXT")"
MYSQL_PASSWORD_ENCODED="$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$MYSQL_PASSWORD")"

# 当前测试库名按账号名设置；如 RDS 使用其他 database，替换最后的 /common_server。
MYSQL_URL="mysql://${MYSQL_USER}:${MYSQL_PASSWORD_ENCODED}@${MYSQL_HOST}:3306/common_server"
ignis service secrets set --service api mysql-url "$MYSQL_URL"
```

## CN 部署

```bash
ignis --region cn whoami
ignis project sync --mode apply
ignis service publish --service api
ignis service deploy --service api <api-version>
ignis service publish --service web
ignis service deploy --service web <web-version>
```

## 验证

部署后入口：

- 前端：`https://prj-87e446f7f183c261.transairobot.com/`
- API health：`https://prj-87e446f7f183c261.transairobot.com/api`
- API increment：`POST https://prj-87e446f7f183c261.transairobot.com/api/increment`
- API transaction smoke：`POST https://prj-87e446f7f183c261.transairobot.com/api/transaction-smoke`
- MySQL health：`https://prj-87e446f7f183c261.transairobot.com/api/mysql`
- MySQL increment：`POST https://prj-87e446f7f183c261.transairobot.com/api/mysql/increment`
- MySQL bulk smoke：`POST https://prj-87e446f7f183c261.transairobot.com/api/mysql/bulk-smoke`

预期 `GET /api` 返回类似：

```text
postgres=ok
counter=1
events=1
types=ok
```

预期 `GET /api/mysql` 返回类似：

```text
mysql=ok
counter=1
events=1
types=ok
pool=host-side
```
