# `ignis.hcl` Guide

`ignis.hcl` is the project-level manifest used by Ignis. It defines:

- the project name and current public domain
- listeners and public exposures
- the services that belong to the project
- each service's local relative path
- HTTP runtime configuration for `http` services
- static build configuration for `frontend` services
- optional async jobs and cron schedules

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

jobs = [
  {
    name = "process_upload"
    queue = "default"
    target = {
      service = "api"
      binding = "http"
      path = "/jobs/process-upload"
      method = "POST"
    }
    timeout_ms = 120000
    retry = {
      max_attempts = 3
      backoff = "exponential"
      initial_delay_ms = 5000
      max_delay_ms = 60000
    }
    concurrency = {
      max_running = 1
    }
  }
]

schedules = [
  {
    name = "nightly_upload_digest"
    job = "process_upload"
    cron = "0 2 * * *"
    timezone = "UTC"
    enabled = true
    overlap_policy = "forbid"
    misfire_policy = "skip"
    input = {
      source = "schedule"
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

### 3.5 `jobs`

`jobs` declares async job types for the project. A job target is an HTTP endpoint on a service in the same project.

Common fields:

- `name`
- `queue`
- `target.service`
- `target.binding`
- `target.path`
- `target.method`
- `timeout_ms`
- `retry.max_attempts`
- `retry.backoff`
- `retry.initial_delay_ms`
- `retry.max_delay_ms`
- `concurrency.max_running`
- `retention.keep_success_days`
- `retention.keep_failed_days`

Current target constraints:

- `target.service` must reference an `http` service.
- `target.binding` currently supports the service's default `http` binding.
- `target.path` must be an absolute path.
- job input must be a JSON object.

### 3.6 `schedules`

`schedules` declares cron-based triggers that enqueue jobs.

Common fields:

- `name`
- `job`
- `cron`
- `timezone`
- `enabled`
- `overlap_policy`
- `misfire_policy`
- `input`

`job` must reference a declared job. `cron` uses 5 fields: `minute hour day month weekday`.

Current policy values:

- `overlap_policy`: `allow`, `forbid`, `replace`
- `misfire_policy`: `skip`, `run_once`, `catch_up`

Read [Jobs and Schedules](./jobs-and-schedules.md) for the runtime behavior and current limitations.

## 4. CLI behavior tied to `ignis.hcl`

- `ignis project create` writes the initial file
- `ignis project sync --mode apply` may write back `project.domain`
- `ignis project sync --mode apply` persists `jobs` and `schedules` to project automation config
- `ignis service new` updates `services`
- service lifecycle commands read the current project and service definitions from this file

## 5. Related documents

- [Quickstart](./quickstart.md)
- [CLI Guide](./cli.md)
- [API Reference](./api.md)
- [Ignis Service Link](./ignis-service-link.md)
- [Jobs and Schedules](./jobs-and-schedules.md)
