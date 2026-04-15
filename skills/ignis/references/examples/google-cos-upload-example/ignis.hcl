project = {
  name = "google-cos-upload-example"
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
    http = {
      component = "target/wasm32-wasip2/release/api.wasm"
      base_path = "/"
    }
    ignis_login = {
      display_name = "google-cos-upload-example"
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
    name = "web"
    kind = "frontend"
    path = "services/web"
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
