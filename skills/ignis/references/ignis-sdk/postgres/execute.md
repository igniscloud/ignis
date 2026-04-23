# function ignis_sdk::postgres::execute

```rust
pub fn execute(sql: &str, params: &[PostgresValue]) -> Result<u64, String>
```

Executes a single SQL statement and returns the number of affected rows.

Use `$1`, `$2`, ... placeholders and pass matching `PostgresValue` parameters.
