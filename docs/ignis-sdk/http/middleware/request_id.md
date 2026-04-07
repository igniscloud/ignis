# function ignis_sdk::http::middleware::request_id

```rust
pub fn request_id () -> Middleware
```

Adds a generated request ID to the request extensions and response
headers.

The ID is exposed to handlers through [`Context::request_id`] and is
returned to clients as `x-request-id`.

