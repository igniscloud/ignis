# function ember_sdk::sqlite::execute_batch

```rust
pub fn execute_batch (sql : & str) -> Result < u64 , String >
```

Executes a batch of SQL statements separated by semicolons.

This is useful for schema setup or other multi-statement initialization
that does not need per-statement parameter binding.

