# Ignis

<p align="center">
  <img src="./docs/assets/ignis-icon.svg" alt="Ignis icon" width="64" height="64" />
</p>

Ignis is a Rust workspace for building and publishing Wasm HTTP services.


## Features

- Rust + Wasm execution path built for extreme performance and tight control over latency budgets
- Designed for microsecond-scale startup overhead and high-density service execution
- Higher freedom than closed serverless stacks: own your routing, runtime boundaries, service layout, and deployment model
- One developer can ship and iterate on systems ambitious enough to target 100M DAU-class applications
- One stack for fullstack products: static frontends, HTTP services, SQLite-backed state, and publish/deploy workflows

## Quick Start

Install the CLI, log in, and add the Ignis skill for Codex:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://igniscloud.dev/i.sh | sh
ignis login
npx -y skills add https://github.com/igniscloud/ignis --agent codex --skill ignis -y --copy
```

That installs the CLI, authenticates your session, and copies the official `ignis` skill into your Codex setup.

## Install

Install the latest stable CLI on macOS or Linux:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://igniscloud.dev/i.sh | sh
```

Install the latest stable CLI on Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://igniscloud.dev/i.ps1 | iex"
```

Install from source after cloning the repo:

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
- Manage the current project domain with `project.domain` and `ignis domain ...`
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
- [opencode-agent-e2e](./examples/opencode-agent-e2e)

## Docs

- [Quickstart](./docs/quickstart.md)
- [Integration Guide](./docs/integration.md)
- [CLI Guide](./docs/cli.md)
- [ignis.hcl Guide](./docs/ignis-hcl.md)
- [Ignis Service Link](./docs/ignis-service-link.md)
- [Agent Service 实现方案](./docs/agent-service-implementation.md)
- [API Reference](./docs/api.md)
- [Ignis SDK Markdown Reference](./docs/ignis-sdk/index.md)
