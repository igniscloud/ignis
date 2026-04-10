project = {
  name = "dual_frontend_login_example"
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
    name = "admin"
    listener = "public"
    service = "admin"
    path = "/admin"
  },
  {
    name = "app"
    listener = "public"
    service = "app"
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
    ignis_login = {
      display_name = "dual-frontend-login-example"
      redirect_path = "/auth/callback"
      providers = ["google"]
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
    name = "app"
    kind = "frontend"
    path = "services/app"
    frontend = {
      build_command = [
        "bash",
        "-lc",
        "rm -rf dist && mkdir -p dist && cp -R src/. dist/",
      ]
      output_dir = "dist"
      spa_fallback = true
    }
  },
  {
    name = "admin"
    kind = "frontend"
    path = "services/admin"
    frontend = {
      build_command = [
        "bash",
        "-lc",
        "rm -rf dist && mkdir -p dist && cp -R src/. dist/",
      ]
      output_dir = "dist"
      spa_fallback = true
    }
  }
]
