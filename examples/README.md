# Examples

This directory contains the smallest public examples for the `ignis` workspace.

## `hello-fullstack`

A minimal project-style example showing:

- one `http` service with a `GET /hello` handler that returns `hello world`
- one Vue frontend service that fetches the backend response and renders it
- a single `ignis.hcl` project with both `api` and `web` services
- same-origin routing with `web=/` and `api=/api` on one project host

## `sqlite-example`

A minimal project-style SQLite example showing:

- one `http` service backed by the built-in SQLite host API
- one frontend service that reads and mutates the counter through same-origin `/api`
- schema initialization through `ignis-sdk::sqlite::migrations`
- a persistent counter exposed through `GET /api` and `POST /api/increment`
- a single `ignis.hcl` project with `web=/` and `api=/api`

## `ignis-login-example`

A minimal `ignis_login` example showing:

- one `http` service with service-level `[services.ignis_login]`
- hosted `IgnisCloud ID /login` startup through `GET /auth/start`
- a callback endpoint at `GET /auth/callback`
- auto-managed `IGNIS_LOGIN_CLIENT_ID` and `IGNIS_LOGIN_CLIENT_SECRET`
- a single `ignis.hcl` project with one service mounted at `/`

## `dual-frontend-login-example`

A multi-frontend login example showing:

- one `http` API service with `ignis_login` and SQLite-backed user persistence
- one Vue user app mounted at `/`
- one Vue admin app mounted at `/admin`
- same-origin login, callback, session, and registered-user listing through `/api`

## `cos-and-jobs-example`

A fullstack Google login, COS upload, and scheduled cleanup example showing:

- one `http` API service with `ignis_login`, SQLite, and `ignis-sdk::object_store`
- one static frontend service mounted at `/`
- browser direct upload to platform-managed COS through backend-issued presigned URLs
- a 10MB per-user quota keyed by the Google user subject
- a cron-triggered job that releases quota for expired pending uploads

## Math Proof Lab multi-agent theorem proof workflow

`math-proof-lab` is a fullstack OpenCode agent architecture example showing:

- one Vue frontend service that starts a strict theorem-proof workflow for a chosen audience level
- one `http` API service that stores workflow state and dispatches TaskPlan child tasks
- one `orchestrator-agent` that dynamically chooses which specialist agents are needed
- five specialist OpenCode agents for literature and knowledge graph retrieval, formal verification, curriculum mapping, pedagogy, and rigor review
- an explicit rule that audience-friendly explanations must preserve proof status and named black boxes

## Validation

```bash
git clone https://github.com/igniscloud/ignis.git
cd ignis
cargo check --manifest-path examples/hello-fullstack/services/api/Cargo.toml
cargo check --manifest-path examples/sqlite-example/services/api/Cargo.toml
cargo check --manifest-path examples/ignis-login-example/services/api/Cargo.toml
cargo check --manifest-path examples/dual-frontend-login-example/services/api/Cargo.toml
cargo check --manifest-path examples/cos-and-jobs-example/services/api/Cargo.toml --target wasm32-wasip2
cargo check --manifest-path examples/math-proof-lab/services/api/Cargo.toml --target wasm32-wasip2
```
