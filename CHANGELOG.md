# Changelog

All notable user-facing changes to the Ignis CLI release path should be documented in this file.

## Unreleased

- No unreleased stable CLI changes yet.

## 0.1.1 - 2026-04-11

### Added

- Short install redirects on `igniscloud.dev` for the shell and PowerShell installers

### Changed

- Public install docs now prefer `https://igniscloud.dev/i.sh` and `https://igniscloud.dev/i.ps1`

## 0.1.0 - 2026-04-11

### Added

- Stable-only GitHub Releases for `ignis-cli` using `cargo-dist`
- Cross-platform CLI archives for macOS, Linux, and Windows
- Generated shell and PowerShell installers for one-command CLI installation
- A tag-driven release workflow for `v0.1.0` and future stable tags

### Changed

- Public install instructions now prefer prebuilt stable binaries over `cargo install --git`
- Publishing guidance now treats `ignis-cli` as the only released artifact in this repository
