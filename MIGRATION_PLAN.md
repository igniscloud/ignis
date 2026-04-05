# Ember Migration Plan

## Goal

Create a new open-source workspace named `ember` that extracts the reusable Wasm SDK, runtime, and
developer CLI from the private platform repository, while leaving platform control-plane and node
orchestration private for now.

## Scope

Included in the new workspace:

- `ember-cli`
- `ember-host-abi`
- `ember-manifest`
- `ember-sdk`
- `ember-runtime`
- minimal examples for HTTP and SQLite workers

Left in the private platform repository for now:

- control-plane
- node-agent
- deployment RPC and app lifecycle orchestration

## Current Source Mapping

- `wasm_cloud/crates/wkr-cli` -> `ember/crates/ember-cli`
- `wasm_cloud/crates/wkr-manifest` -> `ember/crates/ember-manifest`
- `wasm_cloud/crates/wkr-sdk` -> `ember/crates/ember-sdk`
- `wasm_cloud/crates/wkr-runtime` -> `ember/crates/ember-runtime`
- `wasm_cloud/crates/wkr-sdk/wit/world.wit` -> `ember/crates/ember-host-abi/wit/world.wit`

## Migration Stages

### Stage 1: Workspace bootstrap

- create the standalone `ember` workspace
- copy `manifest`, `sdk`, `runtime`, and the smallest examples
- rename packages from `wkr-*` to `ember-*`
- introduce `ember-host-abi` as the canonical home for the WIT contract

### Stage 2: Dependency cleanup

- make `ember-runtime` stop depending on the guest SDK crate directly
- switch runtime WIT binding generation to use `ember-host-abi`
- switch examples to use `ember-sdk`
- verify the new workspace compiles independently

### Stage 3: Runtime / host separation

- split platform-specific host implementations out of `ember-runtime`
- move `SqliteHost` and future platform host imports into a dedicated host crate
- keep `ember-runtime` focused on engine, store, linking, dispatch, limits, and network policy

### Stage 4: Packaging hardening

- make host ABI consumption publish-friendly instead of relying on sibling relative paths
- decide whether bindings are generated from vendored WIT copies or from a dedicated ABI crate build step
- add public README, examples, and API documentation

## Design Decisions

- One open-source workspace is preferred over multiple repos to keep versioning simple.
- `ember-runtime` should be the reusable execution core, not the full platform host.
- `ember-sdk` should remain guest-facing and ergonomic.
- The WIT contract should live in a neutral ABI crate instead of the SDK crate.

## Immediate TODOs

- [x] create `ember` workspace
- [x] copy the initial crates and examples
- [x] introduce `ember-host-abi`
- [x] rename imports and package names consistently
- [x] compile-check the new workspace
- [x] split SQLite host implementation out of `ember-runtime`
- [x] migrate the CLI into `ember`
- [ ] generalize runtime/host integration beyond the first dedicated host crate
- [ ] add public-facing repository docs

## Notes

- The first migration pass is intentionally non-destructive. The private `wasm_cloud` workspace keeps
  working while `ember` is brought up in parallel.
- Publishing ergonomics for the ABI crate are still a follow-up item after local compilation is stable.
- The first host split is now done via `ember-platform-host`; the next refactor can make host
  integration more generic if needed.

## Validation

- `cargo check --manifest-path /home/hy/workplace/ember/Cargo.toml`
- `cargo check --manifest-path /home/hy/workplace/ember/Cargo.toml -p ember-cli`
- `cargo check --manifest-path /home/hy/workplace/ember/examples/hello-worker/Cargo.toml`
- `cargo check --manifest-path /home/hy/workplace/ember/examples/sqlite-worker/Cargo.toml`
