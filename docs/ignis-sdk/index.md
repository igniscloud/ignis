# module ignis_sdk

Guest-side Rust SDK for Ignis workers.

The crate currently provides:
- `http`: lightweight routing, middleware, and response helpers
- `sqlite`: guest wrappers around the shared host ABI

## Modules

- [`sqlite`](sqlite/index.md) (module): SQLite bindings and migration helpers exposed to guest workers.
- [`http`](http/index.md) (module): Lightweight HTTP routing, middleware, and response helpers for guest

