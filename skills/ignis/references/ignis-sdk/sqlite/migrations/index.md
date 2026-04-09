# module ignis_sdk::sqlite::migrations

```rust
pub mod migrations { ... }
```

Helpers for idempotent schema migrations stored in SQLite itself.

## Types

- [`Migration`](Migration.md) (struct): Describes a single schema migration.

## Functions

- [`apply`](apply.md) (function): Applies any migrations whose `id` has not been recorded yet.

