# function ignis_sdk::postgres::query

```rust
pub fn query(sql: &str, params: &[PostgresValue]) -> Result<QueryResult, String>
```

Executes a query and returns rows in the host ABI format.

`QueryResult` contains `columns` and `rows`; row values use `PostgresValue`.
