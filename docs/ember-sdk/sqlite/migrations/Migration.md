# struct ember_sdk::sqlite::migrations::Migration

```rust
pub struct Migration { ... }
```

Describes a single schema migration.

Migrations are tracked by `id` inside the `_ember_migrations` table
and executed in the order they are provided to [`apply`].

## Fields

### `id`

```rust
pub id: & 'static str
```

Stable unique identifier for the migration.

### `sql`

```rust
pub sql: & 'static str
```

SQL to execute when the migration has not been applied yet.

