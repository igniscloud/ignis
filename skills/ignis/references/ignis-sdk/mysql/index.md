# module ignis_sdk::mysql

MySQL bindings exposed to guest workers.

These APIs forward to a host-side `sqlx::MySqlPool`, keyed by the configured
database URL and pool settings. The Wasm guest never opens TCP sockets itself
and does not manage the connection pool.

Configure an `http` service with an `IGNIS_MYSQL_URL` secret or env value:

```hcl
env = {
  IGNIS_MYSQL_MAX_CONNECTIONS = "64"
  IGNIS_MYSQL_MIN_CONNECTIONS = "4"
}
secrets = {
  IGNIS_MYSQL_URL = "secret://mysql-url"
}
```

The URL format is:

```text
mysql://user:password@host:3306/database
```

Optional pool tuning env values:

- `IGNIS_MYSQL_MAX_CONNECTIONS`
- `IGNIS_MYSQL_MIN_CONNECTIONS`
- `IGNIS_MYSQL_ACQUIRE_TIMEOUT_MS`
- `IGNIS_MYSQL_IDLE_TIMEOUT_MS`
- `IGNIS_MYSQL_MAX_LIFETIME_MS`

## Functions

- [`execute`](execute.md) (function): Executes a single SQL statement and returns the number of affected rows.
- [`query`](query.md) (function): Executes a query and returns rows in the host ABI format.
- [`transaction`](transaction.md) (function): Executes multiple statements inside a single transaction.

## Re-exports

- `MysqlValue`
- `QueryResult`
- `Row`
- `Statement`
