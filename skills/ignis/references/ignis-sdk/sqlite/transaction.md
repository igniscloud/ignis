# function ignis_sdk::sqlite::transaction

```rust
pub fn transaction (statements : & [Statement]) -> Result < u64 , String >
```

Executes multiple prepared statements inside a single transaction.

The host guarantees that either all statements are committed or the
whole transaction is rolled back.

