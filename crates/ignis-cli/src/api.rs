use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use base64::Engine;
use reqwest::multipart::{Form, Part};
use serde_json::{Value, json};
use sha2::Digest;

use crate::config::CliConfig;
use ignis_manifest::{ComponentSignature, ServiceManifest};

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

    pub async fn whoami(&self) -> Result<Value> {
        self.request(
            self.http
                .get(self.url("/v1/whoami"))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn projects(&self) -> Result<Value> {
        self.request(
            self.http
                .get(self.url("/v1/projects"))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn create_project(&self, project: &str) -> Result<Value> {
        self.request(
            self.http
                .post(self.url("/v1/projects"))
                .bearer_auth(&self.config.token)
                .json(&json!({ "project_name": project })),
        )
        .await
    }

    pub async fn project_status(&self, project: &str) -> Result<Value> {
        self.request(
            self.http
                .get(self.url(&format!("/v1/projects/{project}")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn delete_project(&self, project: &str) -> Result<Value> {
        self.request(
            self.http
                .delete(self.url(&format!("/v1/projects/{project}")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn create_project_token(
        &self,
        project: &str,
        issued_for: Option<&str>,
    ) -> Result<Value> {
        self.request(
            self.http
                .post(self.url(&format!("/v1/projects/{project}/tokens")))
                .bearer_auth(&self.config.token)
                .json(&json!({ "issued_for": issued_for })),
        )
        .await
    }

    pub async fn revoke_project_token(&self, project: &str, token_id: &str) -> Result<Value> {
        self.request(
            self.http
                .delete(self.url(&format!("/v1/projects/{project}/tokens/{token_id}")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn create_service(&self, project: &str, service: &ServiceManifest) -> Result<Value> {
        self.request(
            self.http
                .post(self.url(&format!("/v1/projects/{project}/services")))
                .bearer_auth(&self.config.token)
                .json(service),
        )
        .await
    }

    pub async fn service_status(&self, project: &str, service: &str) -> Result<Value> {
        self.request(
            self.http
                .get(self.url(&format!("/v1/projects/{project}/services/{service}")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn publish_http_service(
        &self,
        project: &str,
        service: &str,
        service_manifest: &ServiceManifest,
        artifact_path: &Path,
        component_signature: Option<ComponentSignature>,
        build_metadata: BTreeMap<String, String>,
    ) -> Result<Value> {
        let component = fs::read(artifact_path)
            .with_context(|| format!("reading {}", artifact_path.display()))?;
        let component_sha256 = hex::encode(sha2::Sha256::digest(&component));
        let service_manifest =
            serde_json::to_vec(service_manifest).context("serializing service manifest")?;
        let build_metadata =
            serde_json::to_vec(&build_metadata).context("serializing build metadata")?;

        let mut form = Form::new()
            .part(
                "service_manifest",
                Part::bytes(service_manifest).mime_str("application/json")?,
            )
            .part(
                "build_metadata",
                Part::bytes(build_metadata).mime_str("application/json")?,
            )
            .text("component_sha256", component_sha256)
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
                .post(self.url(&format!(
                    "/v1/projects/{project}/services/{service}/versions"
                )))
                .bearer_auth(&self.config.token)
                .multipart(form),
        )
        .await
    }

    pub async fn publish_frontend_service(
        &self,
        project: &str,
        service: &str,
        service_manifest: &ServiceManifest,
        bundle_path: &Path,
        build_metadata: BTreeMap<String, String>,
    ) -> Result<Value> {
        let bundle =
            fs::read(bundle_path).with_context(|| format!("reading {}", bundle_path.display()))?;
        let service_manifest =
            serde_json::to_vec(service_manifest).context("serializing service manifest")?;
        let build_metadata =
            serde_json::to_vec(&build_metadata).context("serializing build metadata")?;

        let form = Form::new()
            .part(
                "service_manifest",
                Part::bytes(service_manifest).mime_str("application/json")?,
            )
            .part(
                "build_metadata",
                Part::bytes(build_metadata).mime_str("application/json")?,
            )
            .part(
                "site_bundle",
                Part::bytes(bundle)
                    .file_name("site.tar.gz".to_owned())
                    .mime_str("application/gzip")?,
            );

        self.request(
            self.http
                .post(self.url(&format!(
                    "/v1/projects/{project}/services/{service}/versions"
                )))
                .bearer_auth(&self.config.token)
                .multipart(form),
        )
        .await
    }

    pub async fn deploy_service(
        &self,
        project: &str,
        service: &str,
        version: &str,
    ) -> Result<Value> {
        self.request(
            self.http
                .post(self.url(&format!(
                    "/v1/projects/{project}/services/{service}/deployments"
                )))
                .bearer_auth(&self.config.token)
                .json(&json!({ "version": version })),
        )
        .await
    }

    pub async fn deployments(&self, project: &str, service: &str, limit: u32) -> Result<Value> {
        self.request(
            self.http
                .get(self.url(&format!(
                    "/v1/projects/{project}/services/{service}/deployments/history?limit={limit}"
                )))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn events(&self, project: &str, service: &str, limit: u32) -> Result<Value> {
        self.request(
            self.http
                .get(self.url(&format!(
                    "/v1/projects/{project}/services/{service}/events?limit={limit}"
                )))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn logs(&self, project: &str, service: &str, limit: u32) -> Result<Value> {
        self.request(
            self.http
                .get(self.url(&format!(
                    "/v1/projects/{project}/services/{service}/logs?limit={limit}"
                )))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn rollback(&self, project: &str, service: &str, version: &str) -> Result<Value> {
        self.request(
            self.http
                .post(self.url(&format!(
                    "/v1/projects/{project}/services/{service}/rollback"
                )))
                .bearer_auth(&self.config.token)
                .json(&json!({ "version": version })),
        )
        .await
    }

    pub async fn delete_version(
        &self,
        project: &str,
        service: &str,
        version: &str,
    ) -> Result<Value> {
        self.request(
            self.http
                .delete(self.url(&format!(
                    "/v1/projects/{project}/services/{service}/versions/{version}"
                )))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn env_list(&self, project: &str, service: &str) -> Result<Value> {
        self.request(
            self.http
                .get(self.url(&format!("/v1/projects/{project}/services/{service}/env")))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn env_set(
        &self,
        project: &str,
        service: &str,
        name: &str,
        value: &str,
    ) -> Result<Value> {
        self.request(
            self.http
                .post(self.url(&format!("/v1/projects/{project}/services/{service}/env")))
                .bearer_auth(&self.config.token)
                .json(&json!({ "name": name, "value": value })),
        )
        .await
    }

    pub async fn env_delete(&self, project: &str, service: &str, name: &str) -> Result<Value> {
        self.request(
            self.http
                .delete(self.url(&format!(
                    "/v1/projects/{project}/services/{service}/env/{name}"
                )))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn secrets_list(&self, project: &str, service: &str) -> Result<Value> {
        self.request(
            self.http
                .get(self.url(&format!(
                    "/v1/projects/{project}/services/{service}/secrets"
                )))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn secrets_set(
        &self,
        project: &str,
        service: &str,
        name: &str,
        value: &str,
    ) -> Result<Value> {
        self.request(
            self.http
                .post(self.url(&format!(
                    "/v1/projects/{project}/services/{service}/secrets"
                )))
                .bearer_auth(&self.config.token)
                .json(&json!({ "name": name, "value": value })),
        )
        .await
    }

    pub async fn secrets_delete(&self, project: &str, service: &str, name: &str) -> Result<Value> {
        self.request(
            self.http
                .delete(self.url(&format!(
                    "/v1/projects/{project}/services/{service}/secrets/{name}"
                )))
                .bearer_auth(&self.config.token),
        )
        .await
    }

    pub async fn sqlite_backup(&self, project: &str, service: &str) -> Result<Vec<u8>> {
        let response = self
            .http
            .get(self.url(&format!(
                "/v1/projects/{project}/services/{service}/sqlite/backup"
            )))
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

    pub async fn sqlite_restore(
        &self,
        project: &str,
        service: &str,
        bytes: &[u8],
    ) -> Result<Value> {
        self.request(
            self.http
                .post(self.url(&format!(
                    "/v1/projects/{project}/services/{service}/sqlite/restore"
                )))
                .bearer_auth(&self.config.token)
                .json(&json!({
                    "sqlite_base64": base64::engine::general_purpose::STANDARD.encode(bytes)
                })),
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
