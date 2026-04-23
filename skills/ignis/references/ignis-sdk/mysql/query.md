# function ignis_sdk::mysql::query

```rust
pub fn query(sql: &str, params: &[MysqlValue]) -> Result<QueryResult, String>
```

Executes a query and returns rows in the host ABI format.

`QueryResult` contains `columns` and `rows`; row values use `MysqlValue`.
