# function ember_sdk::http::text_response

```rust
pub fn text_response (status : StatusCode , body : impl Into < String >) -> Response < Body >
```

Creates a plain-text response with the given HTTP status.

