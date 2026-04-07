# module ember_sdk::http::middleware

```rust
pub mod middleware { ... }
```

Built-in middleware helpers.

## Functions

- [`request_id`](request_id.md) (function): Adds a generated request ID to the request extensions and response
- [`logger`](logger.md) (function): Logs one line per request to standard output.
- [`cors`](cors.md) (function): Applies permissive CORS headers and short-circuits `OPTIONS`

