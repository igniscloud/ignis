//! Platform-specific host implementations for Ignis.
//!
//! This crate currently contains the first extracted host module: SQLite-backed host imports.

use std::path::PathBuf;

use anyhow::Result;
use ignis_manifest::LoadedManifest;
use wasmtime::component::Linker;

mod sqlite_bindings {
    wasmtime::component::bindgen!({
        path: "../ignis-host-abi/wit",
        world: "imports",
    });
}

pub struct SqliteHost {
    enabled: bool,
    database_path: PathBuf,
}

pub trait HostBindings: Sized + Send + 'static {
    fn from_manifest(manifest: &LoadedManifest) -> Result<Self>;

    fn add_to_linker<T>(
        linker: &mut Linker<T>,
        get: fn(&mut T) -> &mut Self,
    ) -> wasmtime::Result<()>;
}

struct SqliteImports;

impl wasmtime::component::HasData for SqliteImports {
    type Data<'a> = &'a mut SqliteHost;
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
}

impl HostBindings for SqliteHost {
    fn from_manifest(manifest: &LoadedManifest) -> Result<Self> {
        Self::new(manifest)
    }

    fn add_to_linker<T>(
        linker: &mut Linker<T>,
        get: fn(&mut T) -> &mut Self,
    ) -> wasmtime::Result<()> {
        sqlite_bindings::ignis::platform::sqlite::add_to_linker::<T, SqliteImports>(linker, get)
    }
}

impl sqlite_bindings::ignis::platform::sqlite::Host for SqliteHost {
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

    fn transaction(
        &mut self,
        statements: Vec<sqlite_bindings::ignis::platform::sqlite::Statement>,
    ) -> std::result::Result<u64, String> {
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
    ) -> std::result::Result<sqlite_bindings::ignis::platform::sqlite::QueryResult, String> {
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
            values.push(sqlite_bindings::ignis::platform::sqlite::Row { values: row_values });
        }
        Ok(sqlite_bindings::ignis::platform::sqlite::QueryResult {
            columns,
            rows: values,
        })
    }

    fn query_typed(
        &mut self,
        sql: String,
        params: Vec<String>,
    ) -> std::result::Result<sqlite_bindings::ignis::platform::sqlite::TypedQueryResult, String>
    {
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
            values.push(sqlite_bindings::ignis::platform::sqlite::TypedRow { values: row_values });
        }
        Ok(sqlite_bindings::ignis::platform::sqlite::TypedQueryResult {
            columns,
            rows: values,
        })
    }
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
) -> std::result::Result<sqlite_bindings::ignis::platform::sqlite::SqliteValue, String> {
    let value = row
        .get_ref(index)
        .map_err(|error| format!("reading sqlite column {index} failed: {error}"))?;
    Ok(match value {
        rusqlite::types::ValueRef::Null => {
            sqlite_bindings::ignis::platform::sqlite::SqliteValue::Null
        }
        rusqlite::types::ValueRef::Integer(value) => {
            sqlite_bindings::ignis::platform::sqlite::SqliteValue::Integer(value)
        }
        rusqlite::types::ValueRef::Real(value) => {
            sqlite_bindings::ignis::platform::sqlite::SqliteValue::Real(value)
        }
        rusqlite::types::ValueRef::Text(value) => {
            sqlite_bindings::ignis::platform::sqlite::SqliteValue::Text(
                String::from_utf8_lossy(value).into_owned(),
            )
        }
        rusqlite::types::ValueRef::Blob(value) => {
            sqlite_bindings::ignis::platform::sqlite::SqliteValue::Blob(value.to_vec())
        }
    })
}
