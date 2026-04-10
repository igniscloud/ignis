# Ignis

Ignis is a Rust workspace for building and publishing Wasm HTTP services.


## Features

- Rust + Wasm execution path built for extreme performance and tight control over latency budgets
- Designed for microsecond-scale startup overhead and high-density service execution
- Higher freedom than closed serverless stacks: own your routing, runtime boundaries, service layout, and deployment model
- One developer can ship and iterate on systems ambitious enough to target 100M DAU-class applications
- One stack for fullstack products: static frontends, HTTP services, SQLite-backed state, and publish/deploy workflows

## Quick Start

Install the CLI, create a new working directory, generate the Codex skill, set the prompt, and run Codex:

```bash
cargo install --git https://github.com/igniscloud/ignis ignis-cli
mkdir video-gif-studio && cd video-gif-studio
ignis gen-skill --format codex
codex exec "Build a video-to-GIF website with Ignis. Users should sign in with a username and password, with no email required. The conversion feature must only be available after login. Use Vue SSG for the frontend. Read the skill first before implementing anything."
```

That will generate both `.codex/skills/ignis/SKILL.md` and `.codex/skills/ignis-login/SKILL.md` in the current directory, so Codex can pick the right Ignis workflow before it starts building.

If you use OpenCode instead of Codex:

```bash
ignis gen-skill --format opencode
```

Use the same prompt.

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
- Generate the bundled `ignis` and `ignis-login` skills for Codex, OpenCode, or raw Markdown

## Generate Skill

Generate the bundled official skills:

```bash
ignis gen-skill --format codex
```

Supported formats:

- `codex` -> `.codex/skills/ignis/SKILL.md` and `.codex/skills/ignis-login/SKILL.md`
- `opencode` -> `.opencode/skills/ignis/SKILL.md` and `.opencode/skills/ignis-login/SKILL.md`
- `raw` -> `./ignis/skill.md` and `./ignis-login/skill.md`

All formats include the required `references/` documents for each skill.

## Examples

- [hello-fullstack](./examples/hello-fullstack)
- [sqlite-example](./examples/sqlite-example)

## Docs

- [Integration Guide](./docs/integration.md)
- [CLI Guide](./docs/cli.md)
- [ignis.hcl Guide](./docs/ignis-hcl.md)
- [Ignis Service Link](./docs/ignis-service-link.md)
- [API Reference](./docs/api.md)
- [Ignis SDK Markdown Reference](./docs/ignis-sdk/index.md)
