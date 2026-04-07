# function ignis_sdk::sqlite::migrations::apply

```rust
pub fn apply (migrations : & [Migration]) -> Result < Vec < String > , String >
```

Applies any migrations whose `id` has not been recorded yet.

The function creates `_ignis_migrations` if needed, runs pending
migrations in a single transaction, and returns the list of IDs that
were applied during this call.

