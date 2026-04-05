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

## Documentation

- `docs/integration.md`
- `docs/api.md`
- `docs/cli.md`
