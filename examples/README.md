# Examples

This directory contains the smallest public examples for the `ignis` workspace.

## `hello-fullstack`

A minimal project-style example showing:

- one `http` service with a `GET /hello` handler that returns `hello world`
- one Vue frontend service that fetches the backend response and renders it
- a single `ignis.toml` project with both `api` and `web` services
- same-origin routing with `web=/` and `api=/api` on one project host

## `sqlite-example`

A minimal project-style SQLite example showing:

- one `http` service backed by the built-in SQLite host API
- one frontend service that reads and mutates the counter through same-origin `/api`
- schema initialization through `ignis-sdk::sqlite::migrations`
- a persistent counter exposed through `GET /api` and `POST /api/increment`
- a single `ignis.toml` project with `web=/` and `api=/api`

## `ignis-login-example`

A minimal `ignis_login` example showing:

- one `http` service with service-level `[services.ignis_login]`
- hosted `IgnisCloud ID /login` startup through `GET /auth/start`
- a callback endpoint at `GET /auth/callback`
- auto-managed `IGNIS_LOGIN_CLIENT_ID` and `IGNIS_LOGIN_CLIENT_SECRET`
- a single `ignis.toml` project with one service mounted at `/`

## Validation

```bash
git clone https://github.com/igniscloud/ignis.git
cd ignis
cargo check --manifest-path examples/hello-fullstack/services/api/Cargo.toml
cargo check --manifest-path examples/sqlite-example/services/api/Cargo.toml
cargo check --manifest-path examples/ignis-login-example/services/api/Cargo.toml
```
