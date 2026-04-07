# module ignis_sdk::sqlite

```rust
pub mod sqlite { ... }
```

SQLite bindings and migration helpers exposed to guest workers.

These APIs forward to the host ABI generated from WIT and are intended for
the default SQLite database mounted for the current worker instance.

## Modules

- [`migrations`](migrations/index.md) (module): Helpers for idempotent schema migrations stored in SQLite itself.

## Functions

- [`execute`](execute.md) (function): Executes a single SQL statement and returns the number of affected rows.
- [`query`](query.md) (function): Executes a query and returns rows in the untyped host ABI format.
- [`execute_batch`](execute_batch.md) (function): Executes a batch of SQL statements separated by semicolons.
- [`transaction`](transaction.md) (function): Executes multiple prepared statements inside a single transaction.
- [`query_typed`](query_typed.md) (function): Executes a query and returns rows in the typed host ABI format.

## Re-exports

- `QueryResult` (`pub use ignis::platform::sqlite::QueryResult;`): Re-exported low-level SQLite result and statement types generated from
- `Row` (`pub use ignis::platform::sqlite::Row;`): Re-exported low-level SQLite result and statement types generated from
- `SqliteValue` (`pub use ignis::platform::sqlite::SqliteValue;`): Re-exported low-level SQLite result and statement types generated from
- `Statement` (`pub use ignis::platform::sqlite::Statement;`): Re-exported low-level SQLite result and statement types generated from
- `TypedQueryResult` (`pub use ignis::platform::sqlite::TypedQueryResult;`): Re-exported low-level SQLite result and statement types generated from
- `TypedRow` (`pub use ignis::platform::sqlite::TypedRow;`): Re-exported low-level SQLite result and statement types generated from

