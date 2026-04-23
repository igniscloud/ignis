project = {
  name = "postgres-example"
  domain = "prj-87e446f7f183c261.transairobot.com"
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
    postgres = {
      enabled = true
    }
    env = {
      IGNIS_MYSQL_MAX_CONNECTIONS = "64"
      IGNIS_MYSQL_MIN_CONNECTIONS = "4"
      IGNIS_MYSQL_ACQUIRE_TIMEOUT_MS = "5000"
      IGNIS_MYSQL_IDLE_TIMEOUT_MS = "30000"
      IGNIS_MYSQL_MAX_LIFETIME_MS = "600000"
    }
    secrets = {
      IGNIS_MYSQL_URL = "secret://mysql-url"
    }
    resources = {
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
