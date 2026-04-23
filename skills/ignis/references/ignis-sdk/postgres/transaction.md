# function ignis_sdk::postgres::transaction

```rust
pub fn transaction(statements: &[Statement]) -> Result<u64, String>
```

Executes multiple statements inside a single host-side transaction.

The host commits all statements together or returns an error.
