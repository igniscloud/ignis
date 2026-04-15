# Jobs and Schedules

Ignis supports project-declared async jobs and cron schedules.

## What Exists Now

- `ignis.hcl` supports top-level `jobs` and `schedules`.
- `ignis project sync --mode apply` persists the project automation config.
- control-plane exposes job and schedule APIs.
- control-plane runs an automation loop every 30 seconds.
- schedules can enqueue jobs when a 5-field cron expression matches the current minute.
- jobs are leased from `queued` to `running`, dispatched through node ingress, and marked `succeeded` or `failed`.
- each job attempt writes a run record.
- failed jobs retry according to the declared retry policy.
- job execution sends stable `x-ignis-*` headers to the target service.
- `concurrency.max_running` is enforced per project and job type.
- `overlap_policy = "forbid"` skips a schedule fire when the same schedule already has an active queued or running job.

## Manifest

Jobs and schedules are declared at the top level of `ignis.hcl`.

```hcl
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
    retention = {
      keep_success_days = 7
      keep_failed_days = 30
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
      digest = true
    }
  }
]
```

The job target must reference an `http` service in the same project. The job input must be a JSON object.

## Job API

```bash
curl -sS -H "Authorization: Bearer $TOKEN" \
  -H "content-type: application/json" \
  -d '{"job_type":"process_upload","input":{"file_id":"file-..."}}' \
  "$CP/v1/projects/$PROJECT/jobs"
```

Available endpoints:

- `POST /v1/projects/{project}/jobs`
- `GET /v1/projects/{project}/jobs?limit=100`
- `GET /v1/projects/{project}/jobs/{job_id}`
- `POST /v1/projects/{project}/jobs/{job_id}/cancel`
- `GET /v1/projects/{project}/jobs/{job_id}/runs`
- `GET /v1/projects/{project}/schedules`
- `GET /v1/projects/{project}/automation`
- `PUT /v1/projects/{project}/automation`

`POST /jobs` accepts:

- `job_type`: declared job name
- `input`: JSON object, default `{}`
- `priority`: optional integer
- `idempotency_key`: optional string, max 160 characters
- `run_at_ms`: optional scheduled run timestamp

## Execution Request

The dispatcher calls the target service through node ingress. Non-`GET` and non-`HEAD` jobs send the job input as a JSON request body.

The request includes:

- `x-ignis-job-id`
- `x-ignis-job-type`
- `x-ignis-job-attempt`
- `x-ignis-job-max-attempts`
- `x-ignis-trigger`
- `x-ignis-schedule-name`, for schedule jobs
- `x-ignis-schedule-fire-time-ms`, for schedule jobs

`x-ignis-trigger` is derived from the idempotency key:

- `manual`
- `schedule`
- `webhook`
- `system`

## Cron Support

The current cron implementation supports 5 fields:

```text
minute hour day month weekday
```

Supported field forms:

- `*`
- `*/n`
- `a,b,c`
- `a-b`
- `a-b/n`

Supported timezones:

- `UTC`, `Etc/UTC`, `Etc/GMT`, `Z`
- fixed offsets like `+08:00` or `-05:00`
- `Asia/Shanghai`, `Asia/Chongqing`, `Asia/Hong_Kong`, `Asia/Singapore`

## Current Limits

- `overlap_policy = "allow"` and `"forbid"` are usable; `"replace"` is parsed but not fully implemented as replacement semantics.
- `misfire_policy` is parsed, but catch-up behavior is still basic.
- cron named months and weekdays are not supported yet.
- full IANA timezone and DST handling are not supported yet.
- there is no frontend job dashboard yet.
