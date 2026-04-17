project = {
  "name" = "opencode-agent-e2e"
  "domain" = "prj-d36af2b718db1a1a.igniscloud.app"
}
listeners = [
  {
    "name" = "public"
    "protocol" = "http"
  }
]
exposes = [
  {
    "name" = "api"
    "listener" = "public"
    "service" = "api"
    "binding" = "http"
    "path" = "/api"
  },
  {
    "name" = "web"
    "listener" = "public"
    "service" = "web"
    "binding" = "frontend"
    "path" = "/"
  }
]
services = [
  {
    "name" = "api"
    "kind" = "http"
    "path" = "services/api"
    "bindings" = [
      {
        "name" = "http"
        "kind" = "http"
      }
    ]
    "http" = {
      "component" = "target/wasm32-wasip2/release/api.wasm"
      "base_path" = "/"
    }
    "frontend" = null
    "ignis_login" = null
    "resources" = {
      "memory_limit_bytes" = 134217728
    }
  },
  {
    "name" = "web"
    "kind" = "frontend"
    "path" = "services/web"
    "bindings" = [
      {
        "name" = "frontend"
        "kind" = "frontend"
      }
    ]
    "http" = null
    "frontend" = {
      "build_command" = [
        "bash",
        "-lc",
        "rm -rf dist && mkdir -p dist && cp -R src/. dist/"
      ]
      "output_dir" = "dist"
      "spa_fallback" = true
    }
    "ignis_login" = null
  },
  {
    "name" = "agent-service"
    "kind" = "agent"
    "agent_runtime" = "opencode"
    "path" = "services/agent-service"
    "resources" = {
      "memory_limit_bytes" = 536870912
    }
  }
]
