# module ignis_sdk

Guest-side Rust SDK for Ignis workers.

The crate currently provides:
- `http`: lightweight routing, middleware, and response helpers
- `sqlite`: guest wrappers around the shared host ABI
- `object_store`: platform-managed presigned upload and download URLs

## Modules

- [`sqlite`](sqlite/index.md) (module): SQLite bindings and migration helpers exposed to guest workers.
- [`http`](http/index.md) (module): Lightweight HTTP routing, middleware, and response helpers for guest
- [`object_store`](object_store/index.md) (module): Platform-managed object-store helpers exposed to guest workers.
