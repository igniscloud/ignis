# struct ember_sdk::http::Next

```rust
pub struct Next { ... }
```

Handle to the remaining middleware chain plus the final route handler.

Middleware receives a [`Next`] and can decide whether to call
[`Next::run`] to continue the chain.

## Inherent Implementations

### `impl Next`

```rust
impl Next
```

#### `run`

```rust
pub async fn run (self , context : Context) -> Response < Body >
```

Runs the next middleware in the chain or the final route handler.

