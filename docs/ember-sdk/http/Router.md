# struct ember_sdk::http::Router

```rust
pub struct Router { ... }
```

In-memory HTTP router for guest worker request handling.

Routes are matched by HTTP method and path. Path segments may use
`:name` syntax for named captures and `*rest` syntax for wildcards.

## Inherent Implementations

### `impl Router`

```rust
impl Router
```

#### `new`

```rust
pub fn new () -> Self
```

Creates an empty router.

#### `use_middleware`

```rust
pub fn use_middleware (& mut self , middleware : Middleware) -> & mut Self
```

Appends a middleware to the router-wide middleware chain.

Middleware runs in registration order for every matched request.

#### `route`

```rust
pub fn route < F , Fut > (& mut self , method : Method , path : & str , handler : F ,) -> Result < & mut Self , String > where F : Fn (Context) -> Fut + 'static , Fut : Future < Output = Response < Body > > + 'static ,
```

Registers a route handler for the given method and path pattern.

Returns an error when the pattern cannot be inserted into the
internal matcher.

#### `get`

```rust
pub fn get < F , Fut > (& mut self , path : & str , handler : F) -> Result < & mut Self , String > where F : Fn (Context) -> Fut + 'static , Fut : Future < Output = Response < Body > > + 'static ,
```

Registers a `GET` handler.

#### `post`

```rust
pub fn post < F , Fut > (& mut self , path : & str , handler : F) -> Result < & mut Self , String > where F : Fn (Context) -> Fut + 'static , Fut : Future < Output = Response < Body > > + 'static ,
```

Registers a `POST` handler.

#### `put`

```rust
pub fn put < F , Fut > (& mut self , path : & str , handler : F) -> Result < & mut Self , String > where F : Fn (Context) -> Fut + 'static , Fut : Future < Output = Response < Body > > + 'static ,
```

Registers a `PUT` handler.

#### `patch`

```rust
pub fn patch < F , Fut > (& mut self , path : & str , handler : F) -> Result < & mut Self , String > where F : Fn (Context) -> Fut + 'static , Fut : Future < Output = Response < Body > > + 'static ,
```

Registers a `PATCH` handler.

#### `delete`

```rust
pub fn delete < F , Fut > (& mut self , path : & str , handler : F) -> Result < & mut Self , String > where F : Fn (Context) -> Fut + 'static , Fut : Future < Output = Response < Body > > + 'static ,
```

Registers a `DELETE` handler.

#### `options`

```rust
pub fn options < F , Fut > (& mut self , path : & str , handler : F) -> Result < & mut Self , String > where F : Fn (Context) -> Fut + 'static , Fut : Future < Output = Response < Body > > + 'static ,
```

Registers an `OPTIONS` handler.

#### `handle`

```rust
pub async fn handle (& self , request : Request < Body >) -> Response < Body >
```

Dispatches a request through route matching and middleware.

Requests that match the path but not the method receive `405 Method
Not Allowed`; unmatched paths receive `404 Not Found`.

