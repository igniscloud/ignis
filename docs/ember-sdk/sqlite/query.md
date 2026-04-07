# function ember_sdk::sqlite::query

```rust
pub fn query (sql : & str , params : & [impl AsRef < str >]) -> Result < QueryResult , String >
```

Executes a query and returns rows in the untyped host ABI format.

Use this when you need direct access to raw SQLite values returned by
the runtime.

