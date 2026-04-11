# Publishing Notes

Ignis currently uses stable-only GitHub Releases for the CLI binary distribution path.

## Scope

- Release only `ignis-cli`
- Do not publish prereleases
- Use `cargo-dist` to build archives, checksums, and shell / PowerShell installers
- Treat `cargo install --path crates/ignis-cli --force` as a developer path, not the primary user install path

## Version Rules

- First stable tag: `v0.1.0`
- Small user-facing fixes: `v0.1.x`
- Medium feature releases: `v0.2.0`, `v0.3.0`, ...
- Large compatibility or product milestones: `v1.0.0`

## Release Gate

Before cutting a stable tag:

- confirm `ignis login -> project create -> service new -> build -> publish -> deploy` still works on a compatible control-plane
- confirm the install docs point to `releases/latest/download/ignis-cli-installer.sh`
- confirm the shell installer and PowerShell installer paths are still correct
- confirm the examples still compile
- confirm no public docs still tell new users to install via `cargo install --git`

## Validation Commands

```bash
cargo check --workspace
cargo check -p ignis-cli
cargo check --manifest-path examples/hello-fullstack/services/api/Cargo.toml
cargo check --manifest-path examples/sqlite-example/services/api/Cargo.toml
cargo check --manifest-path examples/ignis-login-example/services/api/Cargo.toml
dist plan --allow-dirty
```

## Release Flow

```bash
git commit -am "Chore: release 0.1.0"
git tag v0.1.0
git push
git push origin v0.1.0
```

The GitHub `Release` workflow builds the stable CLI archives and publishes:

- `ignis-cli-installer.sh`
- `ignis-cli-installer.ps1`
- platform archives and `.sha256` files

## User Install Path

Public docs should prefer:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/igniscloud/ignis/releases/latest/download/ignis-cli-installer.sh | sh
```
