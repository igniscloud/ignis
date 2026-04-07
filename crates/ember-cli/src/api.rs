use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use base64::Engine;
use reqwest::multipart::{Form, Part};
use serde_json::{Value, json};

use crate::config::CliConfig;
use ember_manifest::{ComponentSignature, LoadedManifest};

pub struct ApiClient {
    http: reqwest::Client,
    config: CliConfig,
}

impl ApiClient {
    pub fn new(config: CliConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }

    pub async fn publish(
        &self,
        app: &str,
        loaded: &LoadedManifest,
        artifact_path: &Path,
        component_signature: Option<ComponentSignature>,
        build_metadata: BTreeMap<String, String>,
    ) -> Result<Value> {
        let component = fs::read(artifact_path)
            .with_context(|| format!("reading {}", artifact_path.display()))?;
        let manifest = serde_json::to_vec(&loaded.manifest).context("serializing manifest")?;
        let build_metadata =
            serde_json::to_vec(&build_metadata).context("serializing build metadata")?;
        let mut form = Form::new()
            .part(
                "manifest",
                Part::bytes(manifest).mime_str("application/json")?,
            )
            .part(
                "build_metadata",
                Part::bytes(build_metadata).mime_str("application/json")?,
            )
            .part(
                "component",
                Part::bytes(component)
                    .file_name(
                        artifact_path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("component.wasm")
                            .to_owned(),
                    )
                    .mime_str("application/wasm")?,
            );
        if let Some(signature) = component_signature {
            let signature =
                serde_json::to_vec(&signature).context("serializing component signature")?;
            form = form.part(
                "signature",
                Part::bytes(signature).mime_str("application/json")?,
            );
        }
        self.request(
            self.http
                .post(self.url(&format!("/v1/apps/{app}/versions")))
                .bearer_auth(&self.config.token)
                .multipart(form),
        )
        .await
    }

    pub async fn deploy(&self, app: &str, version: &str) -> Result<Value> {
        self.request(
            self.http
                .post(self.url(&format!("/v1/apps/{app}/deployments")))
                .bearer_auth(&self.config.token)
                .json(&json!({ "version": version })),
        )
        .await
    }

    pub async fn status(&self, app: &str) -> Result<Value> {
        self.request(
            self.http
                .get(self.url(&format!("/v1/apps/{app}")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn apps(&self) -> Result<Value> {
        self.request(
            self.http
                .get(self.url("/v1/apps"))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn create_app(&self, app: &str) -> Result<Value> {
        self.request(
            self.http
                .post(self.url("/v1/apps"))
                .bearer_auth(&self.config.token)
                .json(&json!({ "app_name": app })),
        )
        .await
    }

    pub async fn nodes(&self) -> Result<Value> {
        self.request(
            self.http
                .get(self.url("/v1/nodes"))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn deployments(&self, app: &str, limit: u32) -> Result<Value> {
        self.request(
            self.http
                .get(self.url(&format!("/v1/apps/{app}/deployments/history?limit={limit}")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn events(&self, app: &str, limit: u32) -> Result<Value> {
        self.request(
            self.http
                .get(self.url(&format!("/v1/apps/{app}/events?limit={limit}")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn env_list(&self, app: &str) -> Result<Value> {
        self.request(
            self.http
                .get(self.url(&format!("/v1/apps/{app}/env")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn env_set(&self, app: &str, name: &str, value: &str) -> Result<Value> {
        self.request(
            self.http
                .post(self.url(&format!("/v1/apps/{app}/env")))
                .bearer_auth(&self.config.token)
                .json(&json!({ "name": name, "value": value })),
        )
        .await
    }

    pub async fn env_delete(&self, app: &str, name: &str) -> Result<Value> {
        self.request(
            self.http
                .delete(self.url(&format!("/v1/apps/{app}/env/{name}")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn secrets_list(&self, app: &str) -> Result<Value> {
        self.request(
            self.http
                .get(self.url(&format!("/v1/apps/{app}/secrets")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn secrets_set(&self, app: &str, name: &str, value: &str) -> Result<Value> {
        self.request(
            self.http
                .post(self.url(&format!("/v1/apps/{app}/secrets")))
                .bearer_auth(&self.config.token)
                .json(&json!({ "name": name, "value": value })),
        )
        .await
    }

    pub async fn secrets_delete(&self, app: &str, name: &str) -> Result<Value> {
        self.request(
            self.http
                .delete(self.url(&format!("/v1/apps/{app}/secrets/{name}")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn logs(&self, app: &str, limit: u32) -> Result<Value> {
        self.request(
            self.http
                .get(self.url(&format!("/v1/apps/{app}/logs?limit={limit}")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn rollback(&self, app: &str, version: &str) -> Result<Value> {
        self.request(
            self.http
                .post(self.url(&format!("/v1/apps/{app}/rollback")))
                .bearer_auth(&self.config.token)
                .json(&json!({ "version": version })),
        )
        .await
    }

    pub async fn delete_version(&self, app: &str, version: &str) -> Result<Value> {
        self.request(
            self.http
                .delete(self.url(&format!("/v1/apps/{app}/versions/{version}")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn delete_app(&self, app: &str) -> Result<Value> {
        self.request(
            self.http
                .delete(self.url(&format!("/v1/apps/{app}")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn sqlite_backup(&self, app: &str) -> Result<Vec<u8>> {
        let response = self
            .http
            .get(self.url(&format!("/v1/apps/{app}/sqlite/backup")))
            .bearer_auth(&self.config.token)
            .send()
            .await
            .context("sending sqlite backup request")?;
        let status = response.status();
        let text = response
            .text()
            .await
            .context("reading sqlite backup response")?;
        if !status.is_success() {
            bail!("request failed with {status}: {text}");
        }
        let value: Value =
            serde_json::from_str(&text).context("parsing sqlite backup response JSON")?;
        let sqlite_base64 = value
            .get("data")
            .and_then(|item| {
                item.get("sqlite_base64").or_else(|| {
                    item.get("data")
                        .and_then(|nested| nested.get("sqlite_base64"))
                })
            })
            .and_then(Value::as_str)
            .context("sqlite backup response missing data.sqlite_base64")?;
        base64::engine::general_purpose::STANDARD
            .decode(sqlite_base64)
            .context("decoding sqlite backup")
    }

    pub async fn sqlite_restore(&self, app: &str, bytes: &[u8]) -> Result<Value> {
        self.request(
            self.http
                .post(self.url(&format!("/v1/apps/{app}/sqlite/restore")))
                .bearer_auth(&self.config.token)
                .json(&json!({
                    "sqlite_base64": base64::engine::general_purpose::STANDARD.encode(bytes)
                })),
        )
        .await
    }

    pub async fn whoami(&self) -> Result<Value> {
        self.request(
            self.http
                .get(self.url("/v1/whoami"))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    async fn request(&self, builder: reqwest::RequestBuilder) -> Result<Value> {
        request_json(builder).await
    }
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.server.trim_end_matches('/'), path)
    }
}

async fn request_json(builder: reqwest::RequestBuilder) -> Result<Value> {
    let response = builder.send().await.context("sending HTTP request")?;
    let status = response.status();
    let text = response.text().await.context("reading HTTP response")?;
    if !status.is_success() {
        bail!("request failed with {status}: {text}");
    }
    if text.trim().is_empty() {
        return Ok(json!({ "status": status.as_u16() }));
    }
    let value = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }));
    Ok(value)
}
