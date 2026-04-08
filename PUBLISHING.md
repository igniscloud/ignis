# Publishing Notes

This workspace is intentionally being prepared for public release in phases.

## Before pushing the repository public

- confirm crate names, descriptions, keywords, and categories
- confirm no private product names remain in public-facing docs
- confirm CLI config/env compatibility is documented before dropping legacy paths
- confirm examples build against the local workspace
- confirm the ABI source of truth is only `crates/ignis-host-abi/wit/world.wit`
- review whether `Cargo.lock` should remain tracked for the repository

## Before publishing crates

- choose the publication order:
  1. `ignis-host-abi`
  2. `ignis-manifest`
  3. `ignis-sdk`
  4. `ignis-platform-host`
  5. `ignis-runtime`
  6. `ignis-cli`
- decide whether each crate should be published immediately or kept `publish = false` until the API is stable
- add crate-level examples and rustdoc where needed
- decide on repository metadata once the public Git remote exists

## Validation Commands

```bash
cargo check --workspace
cargo check -p ignis-cli
cargo check --manifest-path examples/hello-fullstack/services/api/Cargo.toml
cargo check --manifest-path examples/sqlite-example/services/api/Cargo.toml
```
