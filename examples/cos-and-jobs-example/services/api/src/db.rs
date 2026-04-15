use ignis_sdk::sqlite::{self, SqliteValue};

use crate::models::{UploadRecord, UserInfo};
use crate::util::now_ms;

pub(crate) fn ensure_schema() -> std::result::Result<(), String> {
    let _ = sqlite::migrations::apply(&[
        sqlite::migrations::Migration {
            id: "001_create_registered_users",
            sql: "create table if not exists registered_users (
                sub text primary key,
                display_name text not null,
                avatar_url text not null,
                first_seen_at_ms integer not null,
                last_login_at_ms integer not null
            );",
        },
        sqlite::migrations::Migration {
            id: "002_create_uploads",
            sql: "create table if not exists uploads (
                file_id text primary key,
                user_sub text not null,
                filename text not null,
                content_type text not null,
                size_bytes integer not null,
                status text not null,
                created_at_ms integer not null,
                completed_at_ms integer
            );",
        },
        sqlite::migrations::Migration {
            id: "003_uploads_user_index",
            sql: "create index if not exists uploads_user_sub_idx on uploads (user_sub, created_at_ms desc);",
        },
    ])?;
    Ok(())
}

pub(crate) fn upsert_registered_user(user: &UserInfo) -> std::result::Result<(), String> {
    let now = now_ms();
    let nickname = user.display_nickname();
    let avatar_url = user.avatar_url.clone().unwrap_or_default();
    sqlite::execute(
        "insert into registered_users (sub, display_name, avatar_url, first_seen_at_ms, last_login_at_ms)
         values (?, ?, ?, ?, ?)
         on conflict(sub) do update set
           display_name = excluded.display_name,
           avatar_url = excluded.avatar_url,
           last_login_at_ms = excluded.last_login_at_ms",
        &[
            user.sub.as_str(),
            nickname.as_str(),
            avatar_url.as_str(),
            &now.to_string(),
            &now.to_string(),
        ],
    )?;
    Ok(())
}

pub(crate) fn insert_upload_record(
    user_sub: &str,
    file_id: &str,
    filename: &str,
    content_type: &str,
    size_bytes: u64,
) -> std::result::Result<(), String> {
    let now = now_ms();
    sqlite::execute(
        "insert into uploads (file_id, user_sub, filename, content_type, size_bytes, status, created_at_ms, completed_at_ms)
         values (?, ?, ?, ?, ?, 'pending', ?, null)",
        &[
            file_id,
            user_sub,
            filename,
            content_type,
            &size_bytes.to_string(),
            &now.to_string(),
        ],
    )?;
    Ok(())
}

pub(crate) fn mark_upload_completed(
    user_sub: &str,
    file_id: &str,
) -> std::result::Result<UploadRecord, String> {
    let existing = get_user_upload(user_sub, file_id)?;
    let now = now_ms();
    let now_string = now.to_string();
    sqlite::execute(
        "update uploads set status = 'uploaded', completed_at_ms = ? where user_sub = ? and file_id = ?",
        &[now_string.as_str(), user_sub, file_id],
    )?;
    Ok(UploadRecord {
        status: "uploaded".to_owned(),
        completed_at_ms: Some(now),
        ..existing
    })
}

pub(crate) fn user_usage_bytes(user_sub: &str) -> std::result::Result<u64, String> {
    let result = sqlite::query_typed(
        "select coalesce(sum(size_bytes), 0) from uploads where user_sub = ? and status in ('pending', 'uploaded')",
        &[user_sub],
    )?;
    let row = result
        .rows
        .first()
        .ok_or_else(|| "usage row missing".to_owned())?;
    parse_u64(row.values.first(), "used_bytes")
}

pub(crate) fn list_user_uploads(user_sub: &str) -> std::result::Result<Vec<UploadRecord>, String> {
    let result = sqlite::query_typed(
        "select file_id, filename, content_type, size_bytes, status, created_at_ms, completed_at_ms
         from uploads
         where user_sub = ?
         order by created_at_ms desc, file_id asc",
        &[user_sub],
    )?;
    result.rows.iter().map(upload_from_row).collect()
}

pub(crate) fn get_user_upload(
    user_sub: &str,
    file_id: &str,
) -> std::result::Result<UploadRecord, String> {
    let result = sqlite::query_typed(
        "select file_id, filename, content_type, size_bytes, status, created_at_ms, completed_at_ms
         from uploads
         where user_sub = ? and file_id = ?",
        &[user_sub, file_id],
    )?;
    result
        .rows
        .first()
        .map(upload_from_row)
        .transpose()?
        .ok_or_else(|| "upload not found".to_owned())
}

pub(crate) fn pending_upload_cleanup_stats(
    cutoff_ms: i64,
) -> std::result::Result<(u64, u64), String> {
    let cutoff = cutoff_ms.to_string();
    let result = sqlite::query_typed(
        "select count(*), coalesce(sum(size_bytes), 0)
         from uploads
         where status = 'pending' and created_at_ms < ?",
        &[cutoff.as_str()],
    )?;
    let row = result
        .rows
        .first()
        .ok_or_else(|| "cleanup stats row missing".to_owned())?;
    Ok((
        parse_u64(row.values.first(), "expired_uploads")?,
        parse_u64(row.values.get(1), "released_bytes")?,
    ))
}

pub(crate) fn delete_expired_pending_uploads(cutoff_ms: i64) -> std::result::Result<u64, String> {
    let cutoff = cutoff_ms.to_string();
    sqlite::execute(
        "delete from uploads where status = 'pending' and created_at_ms < ?",
        &[cutoff.as_str()],
    )
}

fn upload_from_row(row: &ignis_sdk::sqlite::TypedRow) -> std::result::Result<UploadRecord, String> {
    Ok(UploadRecord {
        file_id: parse_text(row.values.first(), "file_id")?,
        filename: parse_text(row.values.get(1), "filename")?,
        content_type: parse_text(row.values.get(2), "content_type")?,
        size_bytes: parse_u64(row.values.get(3), "size_bytes")?,
        status: parse_text(row.values.get(4), "status")?,
        created_at_ms: parse_i64(row.values.get(5), "created_at_ms")?,
        completed_at_ms: parse_optional_i64(row.values.get(6), "completed_at_ms")?,
    })
}

fn parse_text(value: Option<&SqliteValue>, field: &str) -> std::result::Result<String, String> {
    match value {
        Some(SqliteValue::Text(value)) => Ok(value.clone()),
        Some(other) => Err(format!("unexpected sqlite type for {field}: {other:?}")),
        None => Err(format!("missing sqlite value for {field}")),
    }
}

fn parse_i64(value: Option<&SqliteValue>, field: &str) -> std::result::Result<i64, String> {
    match value {
        Some(SqliteValue::Integer(value)) => Ok(*value),
        Some(other) => Err(format!("unexpected sqlite type for {field}: {other:?}")),
        None => Err(format!("missing sqlite value for {field}")),
    }
}

fn parse_u64(value: Option<&SqliteValue>, field: &str) -> std::result::Result<u64, String> {
    let value = parse_i64(value, field)?;
    u64::try_from(value).map_err(|_| format!("{field} cannot be negative"))
}

fn parse_optional_i64(
    value: Option<&SqliteValue>,
    field: &str,
) -> std::result::Result<Option<i64>, String> {
    match value {
        Some(SqliteValue::Integer(value)) => Ok(Some(*value)),
        Some(SqliteValue::Null) => Ok(None),
        Some(other) => Err(format!("unexpected sqlite type for {field}: {other:?}")),
        None => Err(format!("missing sqlite value for {field}")),
    }
}
