# Ignis

Ignis is a Rust workspace for building and publishing Wasm HTTP services.

It gives you the pieces needed to work on an Ignis project:

- `ignis` CLI for project creation, service scaffolding, build, publish/deploy, and compatible igniscloud APIs
- `ignis.toml` project manifest parsing and validation
- `ignis-sdk` for HTTP routing, middleware, responses, and SQLite access inside services
- `ignis-runtime` for executing `wasi:http` components
- example projects for a fullstack app and a SQLite-backed service

This repository does not include a public control plane implementation. The CLI talks to a compatible external control plane.

## Features

- Rust + Wasm execution path built for extreme performance and tight control over latency budgets
- Designed for microsecond-scale startup overhead and high-density service execution
- Higher freedom than closed serverless stacks: own your routing, runtime boundaries, service layout, and deployment model
- One developer can ship and iterate on systems ambitious enough to target 100M DAU-class applications
- One stack for fullstack products: static frontends, HTTP services, SQLite-backed state, and publish/deploy workflows

## Install

Install without cloning the repo:

```bash
cargo install --git https://github.com/igniscloud/ignis ignis-cli
```

Install after cloning the repo:

```bash
git clone https://github.com/igniscloud/ignis.git
cd ignis
cargo install --path crates/ignis-cli --force
```

Check that the CLI is available:

```bash
ignis --help
```

## What You Can Do

- Create a new project with `ignis project create`
- Add `http` or `frontend` services with `ignis service new`
- Build a service artifact and publish/deploy it with `ignis service build`, `ignis service publish`, and `ignis service deploy`
- Log in and publish/deploy services to a compatible igniscloud environment
- Generate the `ignis-user` skill package for Codex, OpenCode, or raw Markdown

## Generate Skill

Generate the bundled `ignis-user` skill:

```bash
ignis gen-skill --format codex
```

Supported formats:

- `codex` -> `.codex/skills/ignis-user/SKILL.md`
- `opencode` -> `.opencode/skills/ignis-user/SKILL.md`
- `raw` -> `ignis-user/skill.md`

All formats include the required `references/` documents.

## Examples

- [hello-fullstack](./examples/hello-fullstack)
- [sqlite-example](./examples/sqlite-example)

## Docs

- [Integration Guide](./docs/integration.md)
- [CLI Guide](./docs/cli.md)
- [ignis.toml Guide](./docs/ignis-toml.md)
- [API Reference](./docs/api.md)
- [Ignis SDK Markdown Reference](./docs/ignis-sdk/index.md)
