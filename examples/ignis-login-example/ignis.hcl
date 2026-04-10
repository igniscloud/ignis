project = {
  name = "ignis_login_example"
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
      display_name = "ignis-login-example"
      redirect_path = "/auth/callback"
      providers = ["google"]
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
