# Quickstart

Use this guide when you want the shortest path from an empty directory to a deployed Ignis service.

## 1. Install the CLI

macOS / Linux:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://igniscloud.dev/i.sh | sh
```

Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://igniscloud.dev/i.ps1 | iex"
```

Install from source:

```bash
git clone https://github.com/igniscloud/ignis.git
cd ignis
cargo install --path crates/ignis-cli --force
```

Confirm the CLI is available:

```bash
ignis --help
```

## 2. Sign in

```bash
ignis login --region cn
ignis whoami
```

The CLI opens a browser-based sign-in flow and stores the resulting token locally. You can also log in to `global`; tokens are stored per region, and project-local service operations use the region recorded in `.ignis/project.json`.

## 3. Create a project

```bash
ignis project create hello-project
cd hello-project
```

This creates:

- `ignis.hcl`
- `.ignis/project.json`

`ignis.hcl` stores the project manifest. `.ignis/project.json` stores the remote `project_id` that binds your local directory to the control plane.

## 4. Add a service

Create an HTTP service:

```bash
ignis service new --service api --kind http --path services/api
```

Create a frontend service:

```bash
ignis service new --service web --kind frontend --path services/web
```

## 5. Validate, build, publish, and deploy

HTTP service:

```bash
ignis service check --service api
ignis service build --service api
ignis service publish --service api
ignis service deploy --service api <version>
```

Frontend service:

```bash
ignis service build --service web
ignis service publish --service web
ignis service deploy --service web <version>
```

Use the version returned by `ignis service publish`.

## 6. Generate the official skill

Codex:

```bash
ignis gen-skill --format codex
```

OpenCode:

```bash
ignis gen-skill --format opencode
```

Raw Markdown:

```bash
ignis gen-skill --format raw
```

## 7. What to read next

- [CLI Guide](./cli.md)
- [ignis.hcl Guide](./ignis-hcl.md)
- [API Reference](./api.md)
- [Ignis Service Link](./ignis-service-link.md)
