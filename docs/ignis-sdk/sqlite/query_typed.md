# function ignis_sdk::sqlite::query_typed

```rust
pub fn query_typed (sql : & str , params : & [impl AsRef < str >]) -> Result < TypedQueryResult , String >
```

Executes a query and returns rows in the typed host ABI format.

This is a better fit than [`query`] when you want explicit SQLite type
information for each returned column.

