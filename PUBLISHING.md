# Publishing Notes

This workspace is intentionally being prepared for public release in phases.

## Before pushing the repository public

- confirm crate names, descriptions, keywords, and categories
- confirm no private product names remain in public-facing docs
- confirm CLI config/env compatibility is documented before dropping legacy paths
- confirm examples build against the local workspace
- confirm the ABI source of truth is only `crates/ember-host-abi/wit/world.wit`
- review whether `Cargo.lock` should remain tracked for the repository

## Before publishing crates

- choose the publication order:
  1. `ember-host-abi`
  2. `ember-manifest`
  3. `ember-sdk`
  4. `ember-platform-host`
  5. `ember-runtime`
  6. `ember-cli`
- decide whether each crate should be published immediately or kept `publish = false` until the API is stable
- add crate-level examples and rustdoc where needed
- decide on repository metadata once the public Git remote exists

## Validation Commands

```bash
cargo check --manifest-path /home/hy/workplace/ember/Cargo.toml
cargo check --manifest-path /home/hy/workplace/ember/Cargo.toml -p ember-cli
cargo check --manifest-path /home/hy/workplace/ember/examples/hello-worker/Cargo.toml
cargo check --manifest-path /home/hy/workplace/ember/examples/sqlite-worker/Cargo.toml
```
