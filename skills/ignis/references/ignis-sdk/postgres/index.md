# module ignis_sdk::postgres

Postgres bindings exposed to guest workers.

These APIs forward to the host ABI generated from WIT. The platform owns the
service database connection and the guest never receives database credentials.

Enable platform-managed Postgres for an `http` service with:

```hcl
postgres = {
  enabled = true
}
```

## Functions

- [`execute`](execute.md) (function): Executes a single SQL statement and returns the number of affected rows.
- [`query`](query.md) (function): Executes a query and returns rows in the host ABI format.
- [`transaction`](transaction.md) (function): Executes multiple statements inside a single transaction.

## Re-exports

- `PostgresValue`
- `QueryResult`
- `Row`
- `Statement`
