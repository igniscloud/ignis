# Examples

This directory contains the smallest public examples for the `ignis` workspace.

## `hello-worker`

A minimal HTTP worker showing:

- `wasi:http` entrypoint setup
- routing through `ignis-sdk::http::Router`
- middleware usage
- request body and header access

## `sqlite-worker`

A minimal SQLite-backed worker showing:

- schema initialization through `ignis-sdk::sqlite::migrations`
- typed SQLite reads
- state mutation through simple HTTP handlers

## `hello-fullstack`

A minimal project-style example showing:

- one `http` service with a `GET /hello` handler that returns `hello world`
- one Vue frontend service that fetches the backend response and renders it
- a single `ignis.toml` project with both `api` and `web` services
- same-origin routing with `web=/` and `api=/api` on one project host

## `secret-worker`

A minimal secret-backed worker showing:

- environment-backed secret injection
- direct `wasi:http` request handling without extra framework code
- how to read platform-provided secrets at runtime

## `pocket-tasks-worker`

A fuller SQLite-backed worker showing:

- CRUD routes through `ignis-sdk::http::Router`
- request body parsing and JSON responses
- persistent task storage on the built-in SQLite host API
- a realistic API shape that can back a small frontend

## Validation

```bash
git clone https://github.com/igniscloud/ignis.git
cd ignis
cargo check --manifest-path examples/hello-worker/Cargo.toml
cargo check --manifest-path examples/sqlite-worker/Cargo.toml
cargo check --manifest-path examples/secret-worker/Cargo.toml
cargo check --manifest-path examples/pocket-tasks-worker/Cargo.toml
cargo check --manifest-path examples/hello-fullstack/services/api/Cargo.toml
```
