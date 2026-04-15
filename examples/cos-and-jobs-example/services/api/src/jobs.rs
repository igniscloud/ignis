use ignis_sdk::http::Context;
use wstd::http::{Body, Response, StatusCode};

use crate::constants::{CLEANUP_JOB_TYPE, PENDING_UPLOAD_TTL_MS};
use crate::db::{delete_expired_pending_uploads, ensure_schema, pending_upload_cleanup_stats};
use crate::models::{CleanupPendingUploadsPayload, JobRequestContext};
use crate::response::{json_error, json_response};
use crate::util::{header_value, now_ms};

pub(crate) async fn handle_cleanup_pending_uploads(context: Context) -> Response<Body> {
    if let Err(error) = ensure_schema() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    let job = match require_job_request(&context, CLEANUP_JOB_TYPE) {
        Ok(job) => job,
        Err(error) => return json_error(StatusCode::UNAUTHORIZED, &error),
    };
    let cutoff_ms = now_ms().saturating_sub(PENDING_UPLOAD_TTL_MS);
    let (expired_uploads, released_bytes) = match pending_upload_cleanup_stats(cutoff_ms) {
        Ok(stats) => stats,
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error),
    };
    let deleted_uploads = match delete_expired_pending_uploads(cutoff_ms) {
        Ok(count) => count,
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error),
    };
    json_response(
        StatusCode::OK,
        CleanupPendingUploadsPayload {
            ok: true,
            job_id: job.job_id,
            trigger: job.trigger,
            cutoff_ms,
            older_than_ms: PENDING_UPLOAD_TTL_MS,
            expired_uploads,
            deleted_uploads,
            released_bytes,
        },
    )
}

fn require_job_request(
    context: &Context,
    expected_job_type: &str,
) -> std::result::Result<JobRequestContext, String> {
    let headers = context.request().headers();
    let job_id = header_value(headers, "x-ignis-job-id")
        .ok_or_else(|| "missing x-ignis-job-id".to_owned())?;
    let job_type = header_value(headers, "x-ignis-job-type")
        .ok_or_else(|| "missing x-ignis-job-type".to_owned())?;
    if job_type != expected_job_type {
        return Err(format!(
            "unexpected x-ignis-job-type `{job_type}`, expected `{expected_job_type}`"
        ));
    }
    let trigger = header_value(headers, "x-ignis-trigger").unwrap_or_else(|| "manual".to_owned());
    let attempt = header_value(headers, "x-ignis-job-attempt")
        .ok_or_else(|| "missing x-ignis-job-attempt".to_owned())?;
    let max_attempts = header_value(headers, "x-ignis-job-max-attempts")
        .ok_or_else(|| "missing x-ignis-job-max-attempts".to_owned())?;
    if job_id.trim().is_empty()
        || !matches!(
            trigger.as_str(),
            "manual" | "schedule" | "webhook" | "system"
        )
        || attempt.parse::<u32>().is_err()
        || max_attempts.parse::<u32>().is_err()
    {
        return Err("invalid job execution headers".to_owned());
    }
    Ok(JobRequestContext { job_id, trigger })
}
