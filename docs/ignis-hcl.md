# `ignis.hcl` Guide

`ignis.hcl` is the project-level manifest used by Ignis. It defines:

- the project name and current public domain
- listeners and public exposures
- the services that belong to the project
- each service's local relative path
- HTTP runtime configuration for `http` services
- static build configuration for `frontend` services

The parsed and validated model is owned by `ignis-manifest`. For the exact implementation, use `crates/ignis-manifest/src/project_hcl.rs` as the source of truth.

## 1. Minimal example

```hcl
project = {
  name = "hello-project"
  domain = "prj-1234567890abcdef.transairobot.com"
}

listeners = [
  {
    name = "public"
    protocol = "http"
  }
]

exposes = [
  {
    name = "api"
    listener = "public"
    service = "api"
    path = "/"
  }
]

services = [
  {
    name = "api"
    kind = "http"
    path = "services/api"
    http = {
      component = "target/wasm32-wasip2/release/api.wasm"
      base_path = "/"
    }
  }
]
```

## 2. Full example

```hcl
project = {
  name = "pocket-tasks"
  domain = "pockettasks.transairobot.com"
}

listeners = [
  {
    name = "public"
    protocol = "http"
  }
]

exposes = [
  {
    name = "api"
    listener = "public"
    service = "api"
    path = "/api"
  },
  {
    name = "web"
    listener = "public"
    service = "web"
    path = "/"
  }
]

services = [
  {
    name = "api"
    kind = "http"
    path = "services/api"
    bindings = [
      {
        name = "http"
        kind = "http"
      }
    ]
    http = {
      component = "target/wasm32-wasip2/release/api.wasm"
      base_path = "/api"
    }
    env = {
      APP_NAME = "pocket-tasks"
      LOG_LEVEL = "info"
    }
    secrets = {
      OPENAI_API_KEY = "secret://openai-api-key"
    }
    sqlite = {
      enabled = true
    }
    resources = {
      cpu_time_limit_ms = 5000
      memory_limit_bytes = 134217728
    }
  },
  {
    name = "web"
    kind = "frontend"
    path = "services/web"
    frontend = {
      build_command = ["npm", "run", "build"]
      output_dir = "dist"
      spa_fallback = true
    }
  }
]
```

## 3. Main sections

### 3.1 `project`

`project.name`

- required
- string
- used as the display name and project creation input

`project.domain`

- optional
- host only, without `https://`
- updated by the CLI when project creation, sync, and domain operations change the active public host

### 3.2 `listeners`

Each listener represents a public ingress surface. Today Ignis supports HTTP listeners.

Common fields:

- `name`
- `protocol`

### 3.3 `exposes`

Each exposure maps a service binding onto a public listener path.

Common fields:

- `name`
- `listener`
- `service`
- `binding`
- `path`

### 3.4 `services`

Each service block defines one deployable unit inside the project.

Shared fields:

- `name`
- `kind`
- `path`
- `bindings`
- `env`
- `secrets`
- `sqlite`
- `resources`

`http` services use:

- `http.component`
- `http.base_path`

`frontend` services use:

- `frontend.build_command`
- `frontend.output_dir`
- `frontend.spa_fallback`

## 4. CLI behavior tied to `ignis.hcl`

- `ignis project create` writes the initial file
- `ignis project sync --mode apply` may write back `project.domain`
- `ignis service new` updates `services`
- service lifecycle commands read the current project and service definitions from this file

## 5. Related documents

- [Quickstart](./quickstart.md)
- [CLI Guide](./cli.md)
- [API Reference](./api.md)
- [Ignis Service Link](./ignis-service-link.md)
