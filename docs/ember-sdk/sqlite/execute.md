# function ember_sdk::sqlite::execute

```rust
pub fn execute (sql : & str , params : & [impl AsRef < str >]) -> Result < u64 , String >
```

Executes a single SQL statement and returns the number of affected rows.

`params` are converted to owned strings before crossing the guest/host
ABI boundary.

