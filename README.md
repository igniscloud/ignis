# Ember

`ember` is a small Rust workspace for building and hosting Wasm HTTP workers.

It currently extracts the reusable parts of a larger private platform:

- `ember-cli`: project scaffolding, local build/dev flows, and optional control-plane client commands
- `ember-host-abi`: the WIT contract shared by guest SDKs and host runtimes
- `ember-manifest`: worker manifest parsing, validation, and component signing helpers
- `ember-sdk`: guest-side Rust helpers for HTTP routing and SQLite access
- `ember-runtime`: the reusable Wasmtime-based execution core for `wasi:http` workers
- `ember-platform-host`: the first platform host crate, currently providing SQLite host imports

This repository does not currently include a public control plane or node manager. The CLI is part
of this workspace and can talk to any compatible external control plane.

## Status

The workspace is usable for local development and as a base for a custom platform host, but it is
still in active extraction. The current host split is intentionally conservative: runtime execution
is separated from the first platform host crate, but the host integration model may still evolve.

## Workspace Layout

```text
crates/
  ember-cli/
  ember-host-abi/
  ember-manifest/
  ember-platform-host/
  ember-runtime/
  ember-sdk/
examples/
  hello-worker/
  pocket-tasks-worker/
  secret-worker/
  sqlite-worker/
```

## Getting Started

```bash
git clone https://github.com/pleasewhy/ember.git
cd ember
cargo install --git https://github.com/pleasewhy/ember ember-cli
```

## Quick Validation

```bash
cargo check --workspace
cargo check -p ember-cli
cargo check --manifest-path examples/hello-worker/Cargo.toml
cargo check --manifest-path examples/sqlite-worker/Cargo.toml
cargo check --manifest-path examples/secret-worker/Cargo.toml
cargo check --manifest-path examples/pocket-tasks-worker/Cargo.toml
```

## Documentation

- [Integration Guide](./docs/integration.md)
- [API Reference](./docs/api.md)
- [CLI Guide](./docs/cli.md)

## What Each Crate Does

### `ember-cli`

CLI for:

- `ember init` to scaffold a minimal worker project
- `ember build` and `ember dev` for local iteration
- `ember whoami/publish/deploy/...` for the hosted embercloud control plane

### `ember-sdk`

Guest-side Rust helpers for:

- HTTP routing and middleware
- response helpers
- SQLite calls through the host ABI
- lightweight migrations

### `ember-runtime`

Runtime execution core built on Wasmtime:

- component loading
- WASI / `wasi:http` linking
- request dispatch
- store limits
- CPU time control through epoch interruption
- outbound HTTP policy enforcement

### `ember-platform-host`

Platform host implementations that are not part of the runtime core.

Right now this crate contains the SQLite host implementation and the linker hook used by
`ember-runtime`.

## Current Boundaries

Public in this workspace:

- CLI for local development and compatible control-plane APIs
- guest SDK
- Wasm runtime core
- host ABI
- manifest format and signing helpers
- minimal examples

Not included here:

- app lifecycle APIs
- control plane
- multi-node orchestration

## Next Steps

- make host integration more generic beyond the first SQLite-backed host crate
- add crate-level docs and publish metadata
- add a small public host binary example for local serving

## CLI Quickstart

```bash
git clone https://github.com/pleasewhy/ember.git
cd ember
ember init hello-worker
cd hello-worker
ember build
ember dev --addr 127.0.0.1:3000
```

If you need to publish to embercloud, log in once from the browser and let the CLI keep the
returned token locally:

```bash
ember login
ember whoami
ember publish
ember deploy <version>
```

`ember login` starts a temporary localhost callback, opens the embercloud sign-in page in your
browser, and stores the returned CLI token in the local config. You can still override that saved
session with `--token` or `EMBER_TOKEN` when needed.
