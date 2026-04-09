# struct ignis_sdk::http::Context

```rust
pub struct Context { ... }
```

Request context passed to middleware and route handlers.

It contains the mutable request plus any path parameters extracted by
the router.

## Inherent Implementations

### `impl Context`

```rust
impl Context
```

#### `method`

```rust
pub fn method (& self) -> & Method
```

Returns the HTTP method of the current request.

#### `path`

```rust
pub fn path (& self) -> & str
```

Returns the normalized request path.

#### `request`

```rust
pub fn request (& self) -> & Request < Body >
```

Returns an immutable reference to the underlying request.

#### `request_mut`

```rust
pub fn request_mut (& mut self) -> & mut Request < Body >
```

Returns a mutable reference to the underlying request.

Middleware can use this to attach data in extensions or to mutate
headers before the handler runs.

#### `into_request`

```rust
pub fn into_request (self) -> Request < Body >
```

Consumes the context and returns the owned request.

#### `param`

```rust
pub fn param (& self , name : & str) -> Option < & str >
```

Returns the value of a named path parameter if one was matched.

#### `params`

```rust
pub fn params (& self) -> & BTreeMap < String , String >
```

Returns all matched path parameters.

#### `request_id`

```rust
pub fn request_id (& self) -> Option < & str >
```

Returns the request identifier inserted by
[`middleware::request_id`], if present.

