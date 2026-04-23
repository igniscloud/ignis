# function ignis_sdk::mysql::transaction

```rust
pub fn transaction(statements: &[Statement]) -> Result<u64, String>
```

Executes multiple statements inside a single host-side transaction using the
pooled MySQL connection.

The host commits all statements together or returns an error.
