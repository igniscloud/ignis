# function ignis_sdk::mysql::execute

```rust
pub fn execute(sql: &str, params: &[MysqlValue]) -> Result<u64, String>
```

Executes a single SQL statement and returns the number of affected rows.

Use `?` placeholders and pass matching `MysqlValue` parameters.
