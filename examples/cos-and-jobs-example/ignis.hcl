project = {
  name = "cos-and-jobs-example"
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
      display_name = "cos-and-jobs-example"
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

jobs = [
  {
    name = "cleanup_pending_uploads"
    queue = "default"
    target = {
      service = "api"
      binding = "http"
      path = "/api/jobs/cleanup-pending-uploads"
      method = "POST"
    }
    timeout_ms = 30000
    retry = {
      max_attempts = 3
      backoff = "fixed"
      initial_delay_ms = 60000
      max_delay_ms = 300000
    }
    concurrency = {
      max_running = 1
    }
    retention = {
      keep_success_days = 7
      keep_failed_days = 30
    }
  }
]

schedules = [
  {
    name = "midnight_cleanup_pending_uploads"
    job = "cleanup_pending_uploads"
    cron = "0 0 * * *"
    timezone = "Asia/Shanghai"
    enabled = true
    overlap_policy = "forbid"
    misfire_policy = "skip"
    input = {
      reason = "release_expired_pending_upload_quota"
      max_age_ms = 3600000
    }
  }
]
