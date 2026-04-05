# Examples

This directory contains the smallest public examples for the `ember` workspace.

## `hello-worker`

A minimal HTTP worker showing:

- `wasi:http` entrypoint setup
- routing through `ember-sdk::http::Router`
- middleware usage
- request body and header access

## `sqlite-worker`

A minimal SQLite-backed worker showing:

- schema initialization through `ember-sdk::sqlite::migrations`
- typed SQLite reads
- state mutation through simple HTTP handlers

## Validation

```bash
cargo check --manifest-path /home/hy/workplace/ember/examples/hello-worker/Cargo.toml
cargo check --manifest-path /home/hy/workplace/ember/examples/sqlite-worker/Cargo.toml
```
