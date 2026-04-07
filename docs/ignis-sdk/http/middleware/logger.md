# function ignis_sdk::http::middleware::logger

```rust
pub fn logger () -> Middleware
```

Logs one line per request to standard output.

The log includes method, path, status, duration, and request ID when
available.

