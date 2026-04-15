//! Platform-specific host implementations for Ignis.
//!
//! This crate provides host imports for platform-managed services such as
//! SQLite and object-store presigned URLs.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;
use ignis_manifest::LoadedManifest;
use serde::de::DeserializeOwned;
use wasmtime::component::Linker;

mod platform_bindings {
    wasmtime::component::bindgen!({
        path: "../ignis-host-abi/wit",
        world: "imports",
        imports: { default: async },
    });
}

pub use platform_bindings::ignis::platform::object_store::{
    Header, PresignUploadRequest, PresignedUrl,
};
pub use platform_bindings::ignis::platform::sqlite::{
    QueryResult, Row, SqliteValue, Statement, TypedQueryResult, TypedRow,
};

#[derive(Debug, Clone, Default)]
pub struct HostRuntimeConfig {
    pub object_store: Option<ObjectStoreHostConfig>,
}

#[derive(Debug, Clone)]
pub struct ObjectStoreHostConfig {
    pub control_plane_url: String,
    pub bearer_token: String,
    pub project: String,
}

pub struct PlatformHost {
    sqlite: SqliteHost,
    object_store: ObjectStoreHost,
}

pub struct SqliteHost {
    enabled: bool,
    database_path: PathBuf,
}

#[derive(Clone)]
struct ObjectStoreHost {
    config: Option<ObjectStoreHostConfig>,
    http: reqwest::Client,
}

pub trait HostBindings: Sized + Send + 'static {
    fn from_manifest(manifest: &LoadedManifest, config: &HostRuntimeConfig) -> Result<Self>;

    fn add_to_linker<T>(
        linker: &mut Linker<T>,
        get: fn(&mut T) -> &mut Self,
    ) -> wasmtime::Result<()>
    where
        T: Send;
}

struct PlatformImports;

impl wasmtime::component::HasData for PlatformImports {
    type Data<'a> = &'a mut PlatformHost;
}

impl PlatformHost {
    pub fn new(manifest: &LoadedManifest, config: &HostRuntimeConfig) -> Result<Self> {
        Ok(Self {
            sqlite: SqliteHost::new(manifest)?,
            object_store: ObjectStoreHost::new(config.object_store.clone()),
        })
    }
}

impl HostBindings for PlatformHost {
    fn from_manifest(manifest: &LoadedManifest, config: &HostRuntimeConfig) -> Result<Self> {
        Self::new(manifest, config)
    }

    fn add_to_linker<T>(
        linker: &mut Linker<T>,
        get: fn(&mut T) -> &mut Self,
    ) -> wasmtime::Result<()>
    where
        T: Send,
    {
        platform_bindings::ignis::platform::sqlite::add_to_linker::<T, PlatformImports>(
            linker, get,
        )?;
        platform_bindings::ignis::platform::object_store::add_to_linker::<T, PlatformImports>(
            linker, get,
        )
    }
}

impl SqliteHost {
    pub fn new(manifest: &LoadedManifest) -> Result<Self> {
        Ok(Self {
            enabled: manifest.manifest.sqlite.enabled,
            database_path: sqlite_database_path(manifest),
        })
    }

    fn open_connection(&self) -> std::result::Result<rusqlite::Connection, String> {
        if !self.enabled {
            return Err("sqlite is disabled for this worker".to_owned());
        }
        if let Some(parent) = self.database_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "creating sqlite directory {} failed: {error}",
                    parent.display()
                )
            })?;
        }
        let connection = rusqlite::Connection::open(&self.database_path).map_err(|error| {
            format!(
                "opening sqlite database {} failed: {error}",
                self.database_path.display()
            )
        })?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| format!("configuring sqlite busy timeout failed: {error}"))?;
        Ok(connection)
    }

    fn execute(&mut self, sql: String, params: Vec<String>) -> std::result::Result<u64, String> {
        let connection = self.open_connection()?;
        connection
            .execute(&sql, rusqlite::params_from_iter(params.iter()))
            .map(|count| count as u64)
            .map_err(|error| format!("sqlite execute failed: {error}"))
    }

    fn execute_batch(&mut self, sql: String) -> std::result::Result<u64, String> {
        let connection = self.open_connection()?;
        connection
            .execute_batch(&sql)
            .map_err(|error| format!("sqlite execute_batch failed: {error}"))?;
        Ok(connection.changes())
    }

    fn transaction(&mut self, statements: Vec<Statement>) -> std::result::Result<u64, String> {
        let mut connection = self.open_connection()?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("sqlite begin transaction failed: {error}"))?;
        for statement in &statements {
            transaction
                .execute(
                    &statement.sql,
                    rusqlite::params_from_iter(statement.params.iter()),
                )
                .map_err(|error| format!("sqlite transaction execute failed: {error}"))?;
        }
        let changed = transaction.changes();
        transaction
            .commit()
            .map_err(|error| format!("sqlite commit failed: {error}"))?;
        Ok(changed)
    }

    fn query(
        &mut self,
        sql: String,
        params: Vec<String>,
    ) -> std::result::Result<QueryResult, String> {
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| format!("sqlite prepare failed: {error}"))?;
        let columns = statement
            .column_names()
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        let mut rows = statement
            .query(rusqlite::params_from_iter(params.iter()))
            .map_err(|error| format!("sqlite query failed: {error}"))?;
        let mut values = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|error| format!("sqlite iterating rows failed: {error}"))?
        {
            let mut row_values = Vec::new();
            for index in 0..columns.len() {
                row_values.push(sqlite_value_to_string(row, index)?);
            }
            values.push(Row { values: row_values });
        }
        Ok(QueryResult {
            columns,
            rows: values,
        })
    }

    fn query_typed(
        &mut self,
        sql: String,
        params: Vec<String>,
    ) -> std::result::Result<TypedQueryResult, String> {
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| format!("sqlite prepare failed: {error}"))?;
        let columns = statement
            .column_names()
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        let mut rows = statement
            .query(rusqlite::params_from_iter(params.iter()))
            .map_err(|error| format!("sqlite query failed: {error}"))?;
        let mut values = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|error| format!("sqlite iterating rows failed: {error}"))?
        {
            let mut row_values = Vec::new();
            for index in 0..columns.len() {
                row_values.push(sqlite_value_to_typed(row, index)?);
            }
            values.push(TypedRow { values: row_values });
        }
        Ok(TypedQueryResult {
            columns,
            rows: values,
        })
    }
}

impl ObjectStoreHost {
    fn new(config: Option<ObjectStoreHostConfig>) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    async fn presign_upload(
        &self,
        request: PresignUploadRequest,
    ) -> std::result::Result<PresignedUrl, String> {
        let config = self.config()?;
        let response = self
            .http
            .post(format!(
                "{}/v1/internal/projects/{}/files/presign-upload",
                config.control_plane_url.trim_end_matches('/'),
                config.project
            ))
            .bearer_auth(&config.bearer_token)
            .json(&ApiPresignUploadRequest {
                filename: request.filename,
                content_type: request.content_type,
                size_bytes: request.size_bytes,
                sha256: request.sha256,
                expires_in_ms: request.expires_in_ms,
            })
            .send()
            .await
            .map_err(|error| format!("requesting platform upload presign failed: {error}"))?;
        let data: ApiPresignResponse = decode_envelope(response, "platform upload presign").await?;
        Ok(data.into_presigned_url(true))
    }

    async fn presign_download(
        &self,
        file_id: String,
        expires_in_ms: Option<u64>,
    ) -> std::result::Result<PresignedUrl, String> {
        let config = self.config()?;
        let response = self
            .http
            .post(format!(
                "{}/v1/internal/projects/{}/files/{}/presign-download",
                config.control_plane_url.trim_end_matches('/'),
                config.project,
                file_id
            ))
            .bearer_auth(&config.bearer_token)
            .json(&ApiPresignDownloadRequest { expires_in_ms })
            .send()
            .await
            .map_err(|error| format!("requesting platform download presign failed: {error}"))?;
        let data: ApiPresignResponse =
            decode_envelope(response, "platform download presign").await?;
        Ok(data.into_presigned_url(false))
    }

    fn config(&self) -> std::result::Result<&ObjectStoreHostConfig, String> {
        self.config.as_ref().ok_or_else(|| {
            "object-store presign is unavailable: this runtime is not attached to a project"
                .to_owned()
        })
    }
}

impl platform_bindings::ignis::platform::sqlite::Host for PlatformHost {
    fn execute(
        &mut self,
        sql: String,
        params: Vec<String>,
    ) -> impl std::future::Future<Output = std::result::Result<u64, String>> + Send {
        let result = self.sqlite.execute(sql, params);
        async move { result }
    }

    fn execute_batch(
        &mut self,
        sql: String,
    ) -> impl std::future::Future<Output = std::result::Result<u64, String>> + Send {
        let result = self.sqlite.execute_batch(sql);
        async move { result }
    }

    fn transaction(
        &mut self,
        statements: Vec<Statement>,
    ) -> impl std::future::Future<Output = std::result::Result<u64, String>> + Send {
        let result = self.sqlite.transaction(statements);
        async move { result }
    }

    fn query(
        &mut self,
        sql: String,
        params: Vec<String>,
    ) -> impl std::future::Future<Output = std::result::Result<QueryResult, String>> + Send {
        let result = self.sqlite.query(sql, params);
        async move { result }
    }

    fn query_typed(
        &mut self,
        sql: String,
        params: Vec<String>,
    ) -> impl std::future::Future<Output = std::result::Result<TypedQueryResult, String>> + Send
    {
        let result = self.sqlite.query_typed(sql, params);
        async move { result }
    }
}

impl platform_bindings::ignis::platform::object_store::Host for PlatformHost {
    fn presign_upload(
        &mut self,
        request: PresignUploadRequest,
    ) -> impl std::future::Future<Output = std::result::Result<PresignedUrl, String>> + Send {
        let object_store = self.object_store.clone();
        async move { object_store.presign_upload(request).await }
    }

    fn presign_download(
        &mut self,
        file_id: String,
        expires_in_ms: Option<u64>,
    ) -> impl std::future::Future<Output = std::result::Result<PresignedUrl, String>> + Send {
        let object_store = self.object_store.clone();
        async move { object_store.presign_download(file_id, expires_in_ms).await }
    }
}

#[derive(serde::Serialize)]
struct ApiPresignUploadRequest {
    filename: String,
    content_type: String,
    size_bytes: u64,
    sha256: Option<String>,
    expires_in_ms: Option<u64>,
}

#[derive(serde::Serialize)]
struct ApiPresignDownloadRequest {
    expires_in_ms: Option<u64>,
}

#[derive(serde::Deserialize)]
struct ApiEnvelope<T> {
    data: T,
}

#[derive(serde::Deserialize)]
struct ApiPresignResponse {
    file_id: String,
    #[serde(default)]
    upload_url: Option<String>,
    #[serde(default)]
    download_url: Option<String>,
    method: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    expires_at_ms: Option<u64>,
}

impl ApiPresignResponse {
    fn into_presigned_url(self, upload: bool) -> PresignedUrl {
        let url = if upload {
            self.upload_url.unwrap_or_default()
        } else {
            self.download_url.unwrap_or_default()
        };
        PresignedUrl {
            file_id: self.file_id,
            url,
            method: self.method,
            headers: self
                .headers
                .into_iter()
                .map(|(name, value)| Header { name, value })
                .collect(),
            expires_at_ms: self.expires_at_ms,
        }
    }
}

async fn decode_envelope<T: DeserializeOwned>(
    response: reqwest::Response,
    operation: &str,
) -> std::result::Result<T, String> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("reading {operation} response failed: {error}"))?;
    if !status.is_success() {
        return Err(format!("{operation} failed with {status}: {text}"));
    }
    let envelope: ApiEnvelope<T> = serde_json::from_str(&text)
        .map_err(|error| format!("parsing {operation} response failed: {error}"))?;
    Ok(envelope.data)
}

fn sqlite_database_path(manifest: &LoadedManifest) -> PathBuf {
    if let Some(path) = manifest.manifest.env.get("IGNIS_SQLITE_PATH") {
        return PathBuf::from(path);
    }
    manifest
        .project_dir
        .join(".ignis")
        .join("sqlite")
        .join("default.sqlite3")
}

fn sqlite_value_to_string(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> std::result::Result<String, String> {
    let value = row
        .get_ref(index)
        .map_err(|error| format!("reading sqlite column {index} failed: {error}"))?;
    let text = match value {
        rusqlite::types::ValueRef::Null => String::new(),
        rusqlite::types::ValueRef::Integer(value) => value.to_string(),
        rusqlite::types::ValueRef::Real(value) => value.to_string(),
        rusqlite::types::ValueRef::Text(value) => String::from_utf8_lossy(value).into_owned(),
        rusqlite::types::ValueRef::Blob(value) => format!("0x{}", hex::encode(value)),
    };
    Ok(text)
}

fn sqlite_value_to_typed(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> std::result::Result<SqliteValue, String> {
    let value = row
        .get_ref(index)
        .map_err(|error| format!("reading sqlite column {index} failed: {error}"))?;
    Ok(match value {
        rusqlite::types::ValueRef::Null => SqliteValue::Null,
        rusqlite::types::ValueRef::Integer(value) => SqliteValue::Integer(value),
        rusqlite::types::ValueRef::Real(value) => SqliteValue::Real(value),
        rusqlite::types::ValueRef::Text(value) => {
            SqliteValue::Text(String::from_utf8_lossy(value).into_owned())
        }
        rusqlite::types::ValueRef::Blob(value) => SqliteValue::Blob(value.to_vec()),
    })
}
