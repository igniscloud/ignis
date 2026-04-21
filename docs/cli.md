# CLI Guide

This document explains how to use `ignis-cli` to create projects, add services, build artifacts, and publish to a compatible igniscloud control plane.

The CLI binary name is:

```text
ignis
```

## 1. What the CLI covers

The current CLI supports:

- browser-based sign-in and local token persistence
- generating the official `ignis` and `ignis-login` skills
- creating remote projects and initializing local project roots
- creating `http` and `frontend` services inside a project
- validating common local manifest mistakes
- building, publishing, deploying, and rolling back services
- reading service status, events, deployments, and logs
- managing service-level env, secrets, and SQLite backups

## 2. Install and authenticate

Stable install on macOS / Linux:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://igniscloud.dev/i.sh | sh
```

Stable install on Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://igniscloud.dev/i.ps1 | iex"
```

Install from source:

```bash
git clone https://github.com/igniscloud/ignis.git
cd ignis
cargo install --path crates/ignis-cli --force
```

Check that the CLI is available:

```bash
ignis --help
```

Sign in:

```bash
ignis login --region cn
ignis login --region global
ignis whoami
```

The CLI:

- starts a temporary localhost callback
- opens the browser to the selected region sign-in flow
- prints the sign-in URL to the terminal as a fallback
- stores tokens per region in `$XDG_CONFIG_HOME/ignis/config.toml`
- records the region in `.ignis/project.json` when a project is created or synced
- uses the project region for project-local service operations such as `publish`, `deploy`, `env`, `secrets`, and `sqlite`

If `--region` is omitted, `ignis login` prompts for `cn` or `global` and defaults to `cn`.

Log out:

```bash
ignis logout
```

You can also pass a token explicitly:

```bash
ignis --token <token> whoami
IGNIS_TOKEN=<token> ignis whoami
```

The CLI reads tokens from:

- `IGNIS_TOKEN`
- `IGNISCLOUD_TOKEN`
- `$XDG_CONFIG_HOME/ignis/config.toml`

## 3. Project files

Ignis uses a project-level manifest:

```text
ignis.hcl
```

All `ignis service ...` commands search upward from the current directory until they find `ignis.hcl`.

Ignis also writes local state:

```text
.ignis/project.json
```

This file stores the remote `project_id` returned by the control plane and the project `region` (`cn` or `global`). Remote writes are bound to that `project_id` and use the recorded region, not `project.name`.

`ignis.hcl` also stores the current public host in `project.domain`. Project-local remote service operations use the `.ignis/project.json` region to choose the matching saved account, so a `cn` project publishes and deploys with the `cn` token while a `global` project uses the `global` token.

Read the full manifest model in [the ignis.hcl guide](./ignis-hcl.md).

## 4. Minimal workflow

Minimal HTTP service flow:

```bash
ignis login
ignis project create hello-project
cd hello-project
ignis service new --service api --kind http --path services/api
ignis project sync --mode plan
ignis project sync --mode apply
ignis service check --service api
ignis service build --service api
ignis service publish --service api
ignis service deploy --service api <version>
```

Notes:

- `ignis project create` creates the remote project immediately
- the returned `project_id` is written to `.ignis/project.json`
- `project.domain` is written back into `ignis.hcl`
- `<version>` comes from `ignis service publish`

Minimal frontend flow:

```bash
ignis service new --service web --kind frontend --path services/web
ignis service build --service web
ignis service publish --service web
ignis service deploy --service web <version>
```

Minimal OpenCode agent flow:

```bash
ignis service new \
  --service agent-service \
  --kind agent \
  --runtime opencode \
  --path services/agent-service

cp ~/.config/opencode/opencode.json services/agent-service/opencode.json
chmod 600 services/agent-service/opencode.json

ignis service check --service agent-service
ignis service build --service agent-service
ignis service publish --service agent-service
ignis service deploy --service agent-service <version>
```

Minimal Codex agent flow with local Codex auth files:

```bash
ignis service new \
  --service agent-service \
  --kind agent \
  --path services/agent-service

cp ~/.codex/auth.json ~/.codex/config.toml services/agent-service/
chmod 600 services/agent-service/auth.json services/agent-service/config.toml

ignis service check --service agent-service
ignis service build --service agent-service
ignis service publish --service agent-service
ignis service deploy --service agent-service <version>
```

Use an internal `agent` service when a product requirement needs LLM or agent behavior. Prefer creating tasks through `agent-service` over making direct model-provider HTTP requests from a business `http` service.

## 5. `ignis gen-skill`

Command:

- `ignis gen-skill --format <codex|opencode|raw>`

The CLI generates two bundled official skills:

- `ignis`
- `ignis-login`

Formats:

- `codex` -> `.codex/skills/<skill>/SKILL.md`
- `opencode` -> `.opencode/skills/<skill>/SKILL.md`
- `raw` -> `./<skill>/skill.md`

Examples:

```bash
ignis gen-skill --format codex
ignis gen-skill --format opencode
ignis gen-skill --format raw
```

You can also set a custom output root:

```bash
ignis gen-skill --format codex --path ./internal-skills
```

Use `--force` if the target skill directory already exists.

## 6. `ignis project`

Key commands:

- `ignis project create <name>`
- `ignis project sync --mode <plan|apply>`
- `ignis project list`
- `ignis project status <name>`
- `ignis project delete <name>`
- `ignis project token create <name>`
- `ignis project token revoke <name> <token-id>`

Use `ignis project sync --mode apply` when you have an `ignis.hcl` checkout without `.ignis/project.json` and need to bind the local directory to the remote project.

## 7. `ignis service`

Key commands:

- `ignis service new --service <name> --kind <http|frontend|agent> [--runtime <codex|opencode>] --path <relative-path>`
- `ignis service list`
- `ignis service status --service <name>`
- `ignis service check --service <name>`
- `ignis service delete --service <name>`
- `ignis service build --service <name>`
- `ignis service publish --service <name>`
- `ignis service deploy --service <name> <version>`
- `ignis service logs --service <name>`
- `ignis service rollback --service <name> <version>`
- `ignis service env ...`
- `ignis service secrets ...`
- `ignis service sqlite ...`

All service commands must run inside a project directory.

`service new --kind agent` creates an internal task agent service. The default runtime is Codex. Use `--runtime opencode` for OpenCode:

```bash
ignis service new \
  --service agent-service \
  --kind agent \
  --runtime opencode \
  --path services/agent-service
```

For OpenCode, place `opencode.json` in the service directory before build/publish. Ignis uploads that file as the agent artifact and injects it into the runtime container at `$HOME/.config/opencode/opencode.json`.

Services in the same project call the agent through internal service DNS:

```text
POST http://agent-service.svc/v1/tasks
GET  http://agent-service.svc/v1/tasks/{task_id}
```

## 8. Troubleshooting

Common checks:

- missing `ignis.hcl` -> run the command from a project directory
- missing `.ignis/project.json` -> run `ignis project sync --mode apply`
- `project_domain_mismatch` -> align `project.domain` with the current remote domain
- publish cannot find artifacts -> run `ignis service build` first and verify the configured output path

## 9. Related documents

- [Quickstart](./quickstart.md)
- [API Reference](./api.md)
- [ignis.hcl Guide](./ignis-hcl.md)
- [Ignis Service Link](./ignis-service-link.md)
