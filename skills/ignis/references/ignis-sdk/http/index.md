# module ignis_sdk::http

```rust
pub mod http { ... }
```

Lightweight HTTP routing, middleware, and response helpers for guest
workers.

The module is intentionally small: a [`crate::http::Router`] stores route
handlers, [`crate::http::Context`] exposes the current request plus path
parameters, and middleware can wrap handlers through
[`crate::http::Middleware`] and [`crate::http::Next`].

## Modules

- [`middleware`](middleware/index.md) (module): Built-in middleware helpers.

## Types

- [`Next`](Next.md) (struct): Handle to the remaining middleware chain plus the final route handler.
- [`Middleware`](Middleware.md) (struct): Reusable middleware wrapper constructed by [`middleware()`].
- [`Context`](Context.md) (struct): Request context passed to middleware and route handlers.
- [`Router`](Router.md) (struct): In-memory HTTP router for guest worker request handling.

## Functions

- [`middleware`](middleware/index.md) (function): Wraps an async function into a [`Middleware`] value.
- [`text_response`](text_response.md) (function): Creates a plain-text response with the given HTTP status.
- [`empty_response`](empty_response.md) (function): Creates an empty response with the given HTTP status.

