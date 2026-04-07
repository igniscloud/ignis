# struct ember_sdk::http::Middleware

```rust
pub struct Middleware(Arc < dyn Fn (Context , Next) -> BoxFuture < Response < Body > > + 'static >);
```

Reusable middleware wrapper constructed by [`middleware()`].

## Inherent Implementations

### `impl Middleware`

```rust
impl Middleware
```

#### `run`

```rust
pub async fn run (& self , context : Context , next : Next) -> Response < Body >
```

Executes the middleware for the provided request context.

