# API Reference

This document covers the public API surface around `ignis`. In practice there are two layers:

- Rust crate APIs used inside the workspace
- the HTTP contract expected by `ignis-cli` when it talks to a compatible control plane

Ignis does not ship a public control plane implementation in this repository, so the HTTP portion should be read as an integration contract rather than an in-repo server.

## 1. Rust crate APIs

### 1.1 `ignis-manifest`

`ignis-manifest` owns project manifest parsing, validation, compilation, derived worker manifests, and component signatures.

Important constants:

- `MANIFEST_FILE = "worker.toml"`
- `PROJECT_MANIFEST_FILE = "ignis.hcl"`

Important types:

- `ProjectSpec`
- `ListenerSpec`
- `ExposeSpec`
- `ServiceSpec`
- `BindingSpec`
- `CompiledProjectPlan`
- `CompiledServicePlan`
- `CompiledBindingPlan`
- `CompiledExposurePlan`
- `ServiceActivationPlan`
- `ProjectManifest`
- `ServiceManifest`
- `WorkerManifest`
- `ComponentSignature`
- `TrustedSigner`
- `LoadedProjectManifest`
- `LoadedManifest`

Important capabilities:

- `LoadedProjectManifest::load(path)`
- `LoadedProjectManifest::compiled_plan()`
- `LoadedProjectManifest::find_service(name)`
- `LoadedProjectManifest::service_dir(service)`
- `LoadedProjectManifest::http_service_manifest(name)`
- `LoadedManifest::load(path)`
- `LoadedManifest::component_path()`
- `WorkerManifest::validate()`
- `WorkerManifest::render()`
- `sign_component_with_seed(component, key_id, private_seed_base64)`
- `verify_component_signature(component, signature, trusted_signers)`

For the full manifest model behind `ignis.hcl`, read [the ignis.hcl guide](./ignis-hcl.md).

### 1.2 `ignis-sdk`

`ignis-sdk` is the guest-side Rust SDK. Its main public areas today are:

- `ignis_sdk::http`
- `ignis_sdk::sqlite`
- `ignis_sdk::object_store`

The generated SDK reference remains the source of truth for that API surface:

- [ignis-sdk Markdown reference](./ignis-sdk/index.md)

Common `ignis_sdk::http` types and helpers:

- `Router`
- `Context`
- `Middleware`
- `Next`
- `Router::new()`
- `Router::use_middleware(...)`
- `Router::route(...)`
- `Router::get(...)`
- `Router::post(...)`
- `Router::put(...)`
- `Router::patch(...)`
- `Router::delete(...)`
- `Router::options(...)`
- `Router::handle(req).await`
- `text_response(status, body)`
- `empty_response(status)`

Common `ignis_sdk::sqlite` helpers:

- `execute(sql, params)`
- `query(sql, params)`
- `execute_batch(sql)`
- `transaction(statements)`
- `query_typed(sql, params)`
- `sqlite::migrations::apply(migrations)`

Common `ignis_sdk::object_store` helpers:

- `presign_upload(filename, content_type, size_bytes, sha256, expires_in_ms)`
- `presign_download(file_id, expires_in_ms)`

Read [Object Store Presign](./object-store-presign.md) for the platform-managed COS/S3 presign flow and examples.

### 1.3 `ignis-runtime`

`ignis-runtime` is responsible for loading components, linking WASI and `wasi:http`, dispatching requests, enforcing resource limits, and applying outbound network rules.

Representative types:

- `DevServerConfig`
- `WorkerRuntimeOptions`
- `WorkerRuntime<H = SqliteHost>`

### 1.4 `ignis-platform-host`

`ignis-platform-host` is the first platform host module in this workspace. It provides the host-side behavior needed for SQLite access, platform bindings, and runtime integration.

### 1.5 `ignis-cli`

`ignis-cli` is the user-facing entry point for:

- logging in
- creating and syncing projects
- creating services
- building, publishing, and deploying services
- managing env, secrets, and SQLite operations
- generating the official bundled skills

Read [the CLI guide](./cli.md) for the operational workflow.

## 2. HTTP control-plane contract

`ignis-cli` expects a compatible control plane to expose a stable HTTP contract for:

- browser sign-in handoff and token issuance
- project creation and lookup
- project sync and service provisioning
- publish and deploy flows
- service status, history, events, and logs
- env, secret, and SQLite management
- domain management

The key boundary is that the CLI binds remote writes to the `project_id` stored in `.ignis/project.json`, not to `project.name`.

## 3. How the APIs fit together

The common flow looks like this:

1. `ignis-cli` loads `ignis.hcl`
2. `ignis-manifest` validates and compiles the manifest
3. `ignis-cli` builds or locates the service artifact
4. the control plane accepts publish and deploy operations
5. runtime and node-side components consume the compiled plan and activation payload

## 4. Related documents

- [Quickstart](./quickstart.md)
- [CLI Guide](./cli.md)
- [ignis.hcl Guide](./ignis-hcl.md)
- [Ignis Service Link](./ignis-service-link.md)
- [Object Store Presign](./object-store-presign.md)
