//! Manifest types and helpers for Ignis workers.
//!
//! This crate provides:
//! - `worker.toml` parsing and rendering
//! - manifest validation
//! - component signing and verification helpers

mod project_hcl;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

pub const MANIFEST_FILE: &str = "worker.toml";
pub const PROJECT_MANIFEST_FILE: &str = "ignis.hcl";
pub const MAX_RESOURCE_NAME_LEN: usize = 48;
pub const INTERNAL_ONLY_MANIFEST_PREFIX_BASE: &str = "/_ignis_internal";
pub const IGNIS_LOGIN_IGNISCLOUD_ID_BASE_URL_ENV: &str = "IGNISCLOUD_ID_BASE_URL";
pub const IGNIS_LOGIN_CLIENT_ID_SECRET: &str = "IGNIS_LOGIN_CLIENT_ID";
pub const IGNIS_LOGIN_CLIENT_SECRET_SECRET: &str = "IGNIS_LOGIN_CLIENT_SECRET";
pub const IGNIS_LOGIN_RESERVED_SECRETS: [&str; 2] = [
    IGNIS_LOGIN_CLIENT_ID_SECRET,
    IGNIS_LOGIN_CLIENT_SECRET_SECRET,
];
pub const PUBLISHED_SERVICE_PLAN_BUILD_METADATA_KEY: &str = "ignis.published_service_plan";

pub use project_hcl::{
    BindingKind, BindingSpec, CompiledBindingPlan, CompiledExposurePlan, CompiledProjectPlan,
    CompiledServicePlan, ExposeSpec, ListenerProtocol, ListenerSpec, ProjectSpec,
    PublishedBindingPlan, PublishedExposurePlan, PublishedServicePlan, ResolvedDependencyGraph,
    ServiceActivationPlan, ServiceSpec,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerManifest {
    pub name: String,
    pub component: PathBuf,
    #[serde(default = "default_base_path")]
    pub base_path: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub secrets: BTreeMap<String, String>,
    #[serde(default)]
    pub sqlite: SqliteConfig,
    #[serde(default)]
    pub resources: ResourceConfig,
    #[serde(default, skip_serializing_if = "IgnisCloudConfig::is_empty")]
    pub igniscloud: IgnisCloudConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectManifest {
    pub project: ProjectConfig,
    #[serde(default)]
    pub services: Vec<ServiceManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectConfig {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IgnisLoginConfig {
    pub display_name: String,
    pub redirect_path: String,
    pub providers: Vec<IgnisLoginProvider>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IgnisLoginProvider {
    Google,
    TestPassword,
}

impl IgnisLoginProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::TestPassword => "test_password",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceManifest {
    pub name: String,
    pub kind: ServiceKind,
    pub path: PathBuf,
    #[serde(default = "default_service_prefix")]
    pub prefix: String,
    #[serde(default)]
    pub http: Option<HttpServiceConfig>,
    #[serde(default)]
    pub frontend: Option<FrontendServiceConfig>,
    #[serde(default)]
    pub ignis_login: Option<IgnisLoginConfig>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub secrets: BTreeMap<String, String>,
    #[serde(default)]
    pub sqlite: SqliteConfig,
    #[serde(default)]
    pub resources: ResourceConfig,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceKind {
    Http,
    Frontend,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HttpServiceConfig {
    pub component: PathBuf,
    #[serde(default = "default_base_path")]
    pub base_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrontendServiceConfig {
    #[serde(default)]
    pub build_command: Vec<String>,
    pub output_dir: PathBuf,
    #[serde(default)]
    pub spa_fallback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SqliteConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ResourceConfig {
    #[serde(default)]
    pub cpu_time_limit_ms: Option<u64>,
    #[serde(default)]
    pub memory_limit_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IgnisCloudConfig {
    #[serde(default)]
    pub service: Option<String>,
}

impl IgnisCloudConfig {
    fn is_empty(&self) -> bool {
        self.service
            .as_deref()
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentSignature {
    pub key_id: String,
    pub signature_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedSigner {
    pub key_id: String,
    pub public_key_base64: String,
}

#[derive(Debug, Clone)]
pub struct LoadedManifest {
    pub manifest_path: PathBuf,
    pub project_dir: PathBuf,
    pub manifest: WorkerManifest,
}

#[derive(Debug, Clone)]
pub struct LoadedProjectManifest {
    pub manifest_path: PathBuf,
    pub project_dir: PathBuf,
    pub spec: ProjectSpec,
    pub compiled_plan: CompiledProjectPlan,
    pub manifest: ProjectManifest,
}

fn default_base_path() -> String {
    "/".to_owned()
}

fn default_service_prefix() -> String {
    "/".to_owned()
}

impl WorkerManifest {
    pub fn validate(&self) -> Result<()> {
        validate_resource_name(&self.name, "manifest field `name`")?;
        if self.base_path.is_empty() || !self.base_path.starts_with('/') {
            bail!("manifest field `base_path` must start with '/'");
        }
        if self.component.as_os_str().is_empty() {
            bail!("manifest field `component` cannot be empty");
        }
        if let Some(service) = &self.igniscloud.service {
            validate_resource_name(service, "manifest field `igniscloud.service`")?;
        }
        validate_binding_names(self.env.keys(), "env")?;
        validate_binding_names(self.secrets.keys(), "secrets")?;
        self.resources.validate()?;
        Ok(())
    }

    pub fn render(&self) -> Result<String> {
        toml::to_string_pretty(self).context("rendering worker.toml")
    }
}

impl ProjectManifest {
    pub fn validate(&self) -> Result<()> {
        validate_resource_name(&self.project.name, "project field `name`")?;
        let mut service_names = std::collections::BTreeSet::new();
        let mut service_prefixes = std::collections::BTreeMap::<String, String>::new();
        for service in &self.services {
            service.validate()?;
            if !service_names.insert(service.name.clone()) {
                bail!("project contains duplicate service `{}`", service.name);
            }
            let normalized = normalize_service_prefix(&service.prefix)?;
            if let Some(existing) =
                service_prefixes.insert(normalized.clone(), service.name.clone())
            {
                bail!(
                    "route prefix `{normalized}` is declared by both service `{existing}` and `{}`",
                    service.name
                );
            }
        }
        Ok(())
    }
}

impl LoadedManifest {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let input = path.as_ref();
        let manifest_path = if input.is_dir() {
            input.join(MANIFEST_FILE)
        } else {
            input.to_path_buf()
        };
        let raw = fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading manifest at {}", manifest_path.display()))?;
        let manifest: WorkerManifest = toml::from_str(&raw)
            .with_context(|| format!("parsing manifest at {}", manifest_path.display()))?;
        manifest.validate()?;
        let project_dir = manifest_path
            .parent()
            .ok_or_else(|| anyhow!("manifest path has no parent: {}", manifest_path.display()))?
            .to_path_buf();
        Ok(Self {
            manifest_path,
            project_dir,
            manifest,
        })
    }

    pub fn component_path(&self) -> PathBuf {
        self.project_dir.join(&self.manifest.component)
    }

    pub fn igniscloud_service(&self) -> Option<&str> {
        self.manifest
            .igniscloud
            .service
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn from_parts(
        manifest_path: impl Into<PathBuf>,
        project_dir: impl Into<PathBuf>,
        manifest: WorkerManifest,
    ) -> Result<Self> {
        manifest.validate()?;
        Ok(Self {
            manifest_path: manifest_path.into(),
            project_dir: project_dir.into(),
            manifest,
        })
    }
}

impl ResourceConfig {
    pub fn validate(&self) -> Result<()> {
        if self.cpu_time_limit_ms == Some(0) {
            bail!("manifest field `resources.cpu_time_limit_ms` must be greater than 0");
        }
        if self.memory_limit_bytes == Some(0) {
            bail!("manifest field `resources.memory_limit_bytes` must be greater than 0");
        }
        Ok(())
    }
}

impl ServiceManifest {
    pub fn validate(&self) -> Result<()> {
        validate_resource_name(&self.name, "service field `name`")?;
        validate_relative_service_path(&self.path)?;
        validate_service_prefix(&self.prefix)?;
        match self.kind {
            ServiceKind::Http => {
                let http = self.http.as_ref().ok_or_else(|| {
                    anyhow!("http service `{}` is missing `[services.http]`", self.name)
                })?;
                if self.frontend.is_some() {
                    bail!(
                        "http service `{}` cannot define `[services.frontend]`",
                        self.name
                    );
                }
                if let Some(login) = &self.ignis_login {
                    validate_ignis_login(login, self)?;
                }
                http.validate(&self.name)?;
                validate_binding_names(self.env.keys(), "services.env")?;
                validate_binding_names(self.secrets.keys(), "services.secrets")?;
                self.resources.validate()?;
            }
            ServiceKind::Frontend => {
                let frontend = self.frontend.as_ref().ok_or_else(|| {
                    anyhow!(
                        "frontend service `{}` is missing `[services.frontend]`",
                        self.name
                    )
                })?;
                if self.http.is_some() {
                    bail!(
                        "frontend service `{}` cannot define `[services.http]`",
                        self.name
                    );
                }
                if self.ignis_login.is_some() {
                    bail!(
                        "frontend service `{}` cannot define `[services.ignis_login]`",
                        self.name
                    );
                }
                if !self.env.is_empty() {
                    bail!(
                        "frontend service `{}` cannot define `[services.env]`",
                        self.name
                    );
                }
                if !self.secrets.is_empty() {
                    bail!(
                        "frontend service `{}` cannot define `[services.secrets]`",
                        self.name
                    );
                }
                if self.sqlite.enabled {
                    bail!("frontend service `{}` cannot enable sqlite", self.name);
                }
                if self.resources != ResourceConfig::default() {
                    bail!(
                        "frontend service `{}` cannot define `[services.resources]`",
                        self.name
                    );
                }
                frontend.validate(&self.name)?;
            }
        }
        Ok(())
    }

    pub fn http_worker_manifest(&self) -> Result<WorkerManifest> {
        if self.kind != ServiceKind::Http {
            bail!("service `{}` is not an http service", self.name);
        }
        let http = self
            .http
            .as_ref()
            .ok_or_else(|| anyhow!("http service `{}` is missing `[services.http]`", self.name))?;
        Ok(WorkerManifest {
            name: self.name.clone(),
            component: http.component.clone(),
            base_path: http.base_path.clone(),
            env: self.env.clone(),
            secrets: self.secrets.clone(),
            sqlite: self.sqlite.clone(),
            resources: self.resources.clone(),
            igniscloud: IgnisCloudConfig::default(),
        })
    }
}

impl HttpServiceConfig {
    fn validate(&self, service_name: &str) -> Result<()> {
        if self.component.as_os_str().is_empty() {
            bail!("http service `{service_name}` field `http.component` cannot be empty");
        }
        if self.base_path.is_empty() || !self.base_path.starts_with('/') {
            bail!("http service `{service_name}` field `http.base_path` must start with '/'");
        }
        Ok(())
    }
}

impl FrontendServiceConfig {
    fn validate(&self, service_name: &str) -> Result<()> {
        if self.build_command.is_empty() {
            bail!(
                "frontend service `{service_name}` field `frontend.build_command` cannot be empty"
            );
        }
        if self.output_dir.as_os_str().is_empty() {
            bail!("frontend service `{service_name}` field `frontend.output_dir` cannot be empty");
        }
        Ok(())
    }
}

impl ComponentSignature {
    pub fn validate(&self) -> Result<()> {
        if self.key_id.trim().is_empty() {
            bail!("component signature key_id cannot be empty");
        }
        decode_signature_bytes(&self.signature_base64)?;
        Ok(())
    }
}

impl TrustedSigner {
    pub fn validate(&self) -> Result<()> {
        if self.key_id.trim().is_empty() {
            bail!("trusted signer key_id cannot be empty");
        }
        decode_public_key_bytes(&self.public_key_base64)?;
        Ok(())
    }
}

pub fn sign_component_with_seed(
    component: &[u8],
    key_id: &str,
    private_seed_base64: &str,
) -> Result<ComponentSignature> {
    if key_id.trim().is_empty() {
        bail!("component signature key_id cannot be empty");
    }
    let seed_bytes = decode_private_seed_bytes(private_seed_base64)?;
    let signing_key = SigningKey::from_bytes(&seed_bytes);
    let signature = signing_key.sign(component);
    Ok(ComponentSignature {
        key_id: key_id.trim().to_owned(),
        signature_base64: base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
    })
}

pub fn verify_component_signature(
    component: &[u8],
    signature: &ComponentSignature,
    trusted_signers: &[TrustedSigner],
) -> Result<()> {
    signature.validate()?;
    let signer = trusted_signers
        .iter()
        .find(|item| item.key_id == signature.key_id)
        .ok_or_else(|| anyhow!("trusted signer `{}` is not configured", signature.key_id))?;
    signer.validate()?;
    let verifying_key = VerifyingKey::from_bytes(&decode_public_key_bytes(
        &signer.public_key_base64,
    )?)
    .map_err(|error| {
        anyhow!(
            "parsing public key for signer `{}` failed: {error}",
            signer.key_id
        )
    })?;
    let detached = Signature::from_bytes(&decode_signature_bytes(&signature.signature_base64)?);
    verifying_key.verify(component, &detached).map_err(|error| {
        anyhow!(
            "signature verification failed for signer `{}`: {error}",
            signer.key_id
        )
    })
}

fn validate_binding_names<'a>(
    names: impl Iterator<Item = &'a String>,
    field_name: &str,
) -> Result<()> {
    for name in names {
        let valid = name
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_');
        if !valid {
            bail!(
                "manifest field `{field_name}` contains invalid key `{name}`; use only A-Z, 0-9 and '_'"
            );
        }
    }
    Ok(())
}

fn validate_ignis_login(config: &IgnisLoginConfig, service: &ServiceManifest) -> Result<()> {
    if config.display_name.trim().is_empty() {
        bail!(
            "http service `{}` field `ignis_login.display_name` cannot be empty",
            service.name
        );
    }
    if config.providers.is_empty() {
        bail!(
            "http service `{}` field `ignis_login.providers` cannot be empty",
            service.name
        );
    }
    let mut seen = BTreeSet::new();
    for provider in &config.providers {
        if !seen.insert(provider.as_str()) {
            bail!(
                "http service `{}` field `ignis_login.providers` cannot contain duplicate provider `{}`",
                service.name,
                provider.as_str()
            );
        }
    }
    validate_service_prefix_like_path(
        &config.redirect_path,
        &format!(
            "http service `{}` field `ignis_login.redirect_path`",
            service.name
        ),
    )?;
    if service
        .env
        .contains_key(IGNIS_LOGIN_IGNISCLOUD_ID_BASE_URL_ENV)
    {
        bail!(
            "service `{}` cannot define env `{}`; ignis_login does not provide IGNISCLOUD_ID_BASE_URL as an env var",
            service.name,
            IGNIS_LOGIN_IGNISCLOUD_ID_BASE_URL_ENV
        );
    }
    for reserved in IGNIS_LOGIN_RESERVED_SECRETS {
        if service.env.contains_key(reserved) {
            bail!(
                "service `{}` cannot define reserved ignis_login env `{reserved}`",
                service.name
            );
        }
        if service.secrets.contains_key(reserved) {
            bail!(
                "service `{}` cannot define reserved ignis_login secret `{reserved}`",
                service.name
            );
        }
    }
    Ok(())
}

fn validate_resource_name(name: &str, field_name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        bail!("{field_name} cannot be empty");
    }
    if trimmed.len() > MAX_RESOURCE_NAME_LEN {
        bail!("{field_name} must be at most {MAX_RESOURCE_NAME_LEN} characters long");
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        bail!("{field_name} must contain only letters, numbers, '-' or '_'");
    }
    Ok(())
}

fn validate_relative_service_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        bail!("service field `path` cannot be empty");
    }
    if path.is_absolute() {
        bail!("service field `path` must be relative to the project root");
    }
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            bail!("service field `path` cannot contain `..`");
        }
    }
    Ok(())
}

fn validate_service_prefix(prefix: &str) -> Result<()> {
    normalize_service_prefix(prefix).map(|_| ())
}

fn validate_service_prefix_like_path(value: &str, field_name: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{field_name} cannot be empty");
    }
    if !trimmed.starts_with('/') {
        bail!("{field_name} must start with '/'");
    }
    if trimmed.contains("//") {
        bail!("{field_name} cannot contain empty path segments");
    }
    if trimmed.contains('?') || trimmed.contains('#') {
        bail!("{field_name} cannot contain query or fragment syntax");
    }
    Ok(())
}

fn normalize_service_prefix(prefix: &str) -> Result<String> {
    let trimmed = prefix.trim();
    if trimmed.is_empty() {
        bail!("service field `prefix` cannot be empty");
    }
    if !trimmed.starts_with('/') {
        bail!("service field `prefix` must start with '/'");
    }
    if trimmed.contains("//") {
        bail!("service field `prefix` cannot contain empty path segments");
    }
    if trimmed.contains('?') || trimmed.contains('#') {
        bail!("service field `prefix` cannot contain query or fragment syntax");
    }
    if trimmed != "/" && trimmed.ends_with('/') {
        return Ok(trimmed.trim_end_matches('/').to_owned());
    }
    Ok(trimmed.to_owned())
}

fn decode_private_seed_bytes(value: &str) -> Result<[u8; 32]> {
    decode_fixed_bytes(value, "signing key seed")
}

fn decode_public_key_bytes(value: &str) -> Result<[u8; 32]> {
    decode_fixed_bytes(value, "public key")
}

fn decode_signature_bytes(value: &str) -> Result<[u8; 64]> {
    decode_fixed_bytes(value, "signature")
}

fn decode_fixed_bytes<const N: usize>(value: &str, label: &str) -> Result<[u8; N]> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .with_context(|| format!("decoding {label} from base64"))?;
    raw.try_into()
        .map_err(|_| anyhow!("{label} must decode to exactly {N} bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_manifest_shape() {
        let manifest = WorkerManifest {
            name: "hello_worker".to_owned(),
            component: PathBuf::from("target/wasm32-wasip2/release/hello_worker.wasm"),
            base_path: "/".to_owned(),
            env: BTreeMap::from([(String::from("LOG_LEVEL"), String::from("debug"))]),
            secrets: BTreeMap::from([(String::from("API_KEY"), String::from("secret://api-key"))]),
            sqlite: SqliteConfig { enabled: true },
            resources: ResourceConfig {
                cpu_time_limit_ms: Some(5_000),
                memory_limit_bytes: Some(64 * 1024 * 1024),
            },
            igniscloud: IgnisCloudConfig {
                service: Some("hello-worker".to_owned()),
            },
        };

        manifest.validate().unwrap();
        let rendered = manifest.render().unwrap();
        assert!(rendered.contains("hello_worker"));
        assert!(rendered.contains("enabled = true"));
        assert!(rendered.contains("cpu_time_limit_ms = 5000"));
    }

    #[test]
    fn rejects_invalid_base_path() {
        let manifest = WorkerManifest {
            name: "hello_worker".to_owned(),
            component: PathBuf::from("app.wasm"),
            base_path: "api".to_owned(),
            env: BTreeMap::new(),
            secrets: BTreeMap::new(),
            sqlite: SqliteConfig::default(),
            resources: ResourceConfig::default(),
            igniscloud: IgnisCloudConfig::default(),
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn rejects_overlong_igniscloud_service_name() {
        let manifest = WorkerManifest {
            name: "hello_worker".to_owned(),
            component: PathBuf::from("app.wasm"),
            base_path: "/".to_owned(),
            env: BTreeMap::new(),
            secrets: BTreeMap::new(),
            sqlite: SqliteConfig::default(),
            resources: ResourceConfig::default(),
            igniscloud: IgnisCloudConfig {
                service: Some("a".repeat(MAX_RESOURCE_NAME_LEN + 1)),
            },
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn signs_and_verifies_component() {
        let signature = sign_component_with_seed(
            b"component-bytes",
            "dev",
            "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=",
        )
        .unwrap();

        verify_component_signature(
            b"component-bytes",
            &signature,
            &[TrustedSigner {
                key_id: "dev".to_owned(),
                public_key_base64: "iojj3XQJ8ZX9UtstPLpdcspnCb8dlBIb83SIAbQPb1w=".to_owned(),
            }],
        )
        .unwrap();
    }

    #[test]
    fn validates_project_manifest_shape() {
        let manifest = ProjectManifest {
            project: ProjectConfig {
                name: "my-project".to_owned(),
            },
            services: vec![
                ServiceManifest {
                    name: "api".to_owned(),
                    kind: ServiceKind::Http,
                    path: PathBuf::from("services/api"),
                    prefix: "/api".to_owned(),
                    http: Some(HttpServiceConfig {
                        component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                        base_path: "/".to_owned(),
                    }),
                    frontend: None,
                    ignis_login: None,
                    env: BTreeMap::from([(String::from("APP_ENV"), String::from("production"))]),
                    secrets: BTreeMap::new(),
                    sqlite: SqliteConfig { enabled: true },
                    resources: ResourceConfig {
                        cpu_time_limit_ms: Some(5_000),
                        memory_limit_bytes: Some(64 * 1024 * 1024),
                    },
                },
                ServiceManifest {
                    name: "web".to_owned(),
                    kind: ServiceKind::Frontend,
                    path: PathBuf::from("services/web"),
                    prefix: "/".to_owned(),
                    http: None,
                    frontend: Some(FrontendServiceConfig {
                        build_command: vec!["pnpm".to_owned(), "build".to_owned()],
                        output_dir: PathBuf::from("dist"),
                        spa_fallback: true,
                    }),
                    ignis_login: None,
                    env: BTreeMap::new(),
                    secrets: BTreeMap::new(),
                    sqlite: SqliteConfig::default(),
                    resources: ResourceConfig::default(),
                },
            ],
        };

        manifest.validate().unwrap();
        let rendered = manifest.render().unwrap();
        assert!(rendered.contains("my-project"));
        assert!(rendered.contains("build_command"));
    }

    #[test]
    fn rejects_duplicate_service_prefixes() {
        let manifest = ProjectManifest {
            project: ProjectConfig {
                name: "my-project".to_owned(),
            },
            services: vec![
                ServiceManifest {
                    name: "api".to_owned(),
                    kind: ServiceKind::Http,
                    path: PathBuf::from("services/api"),
                    prefix: "/api".to_owned(),
                    http: Some(HttpServiceConfig {
                        component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                        base_path: "/".to_owned(),
                    }),
                    frontend: None,
                    ignis_login: None,
                    env: BTreeMap::new(),
                    secrets: BTreeMap::new(),
                    sqlite: SqliteConfig::default(),
                    resources: ResourceConfig::default(),
                },
                ServiceManifest {
                    name: "web".to_owned(),
                    kind: ServiceKind::Frontend,
                    path: PathBuf::from("services/web"),
                    prefix: "/api/".to_owned(),
                    http: None,
                    frontend: Some(FrontendServiceConfig {
                        build_command: vec!["pnpm".to_owned(), "build".to_owned()],
                        output_dir: PathBuf::from("dist"),
                        spa_fallback: true,
                    }),
                    ignis_login: None,
                    env: BTreeMap::new(),
                    secrets: BTreeMap::new(),
                    sqlite: SqliteConfig::default(),
                    resources: ResourceConfig::default(),
                },
            ],
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn rejects_frontend_runtime_config() {
        let service = ServiceManifest {
            name: "web".to_owned(),
            kind: ServiceKind::Frontend,
            path: PathBuf::from("services/web"),
            prefix: "/".to_owned(),
            http: None,
            frontend: Some(FrontendServiceConfig {
                build_command: vec!["pnpm".to_owned(), "build".to_owned()],
                output_dir: PathBuf::from("dist"),
                spa_fallback: false,
            }),
            ignis_login: None,
            env: BTreeMap::from([(String::from("APP_ENV"), String::from("production"))]),
            secrets: BTreeMap::new(),
            sqlite: SqliteConfig::default(),
            resources: ResourceConfig::default(),
        };

        assert!(service.validate().is_err());
    }

    #[test]
    fn rejects_invalid_service_prefix() {
        let service = ServiceManifest {
            name: "api".to_owned(),
            kind: ServiceKind::Http,
            path: PathBuf::from("services/api"),
            prefix: "api".to_owned(),
            http: Some(HttpServiceConfig {
                component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                base_path: "/".to_owned(),
            }),
            frontend: None,
            ignis_login: None,
            env: BTreeMap::new(),
            secrets: BTreeMap::new(),
            sqlite: SqliteConfig::default(),
            resources: ResourceConfig::default(),
        };

        assert!(service.validate().is_err());
    }

    #[test]
    fn validates_service_ignis_login_shape() {
        let manifest = ProjectManifest {
            project: ProjectConfig {
                name: "video-gif-studio".to_owned(),
            },
            services: vec![ServiceManifest {
                name: "api".to_owned(),
                kind: ServiceKind::Http,
                path: PathBuf::from("services/api"),
                prefix: "/api".to_owned(),
                http: Some(HttpServiceConfig {
                    component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                    base_path: "/".to_owned(),
                }),
                frontend: None,
                ignis_login: Some(IgnisLoginConfig {
                    display_name: "Video GIF Studio".to_owned(),
                    redirect_path: "/auth/common/callback".to_owned(),
                    providers: vec![IgnisLoginProvider::Google, IgnisLoginProvider::TestPassword],
                }),
                env: BTreeMap::new(),
                secrets: BTreeMap::new(),
                sqlite: SqliteConfig::default(),
                resources: ResourceConfig::default(),
            }],
        };

        manifest.validate().unwrap();
        let rendered = manifest.render().unwrap();
        assert!(rendered.contains("ignis_login"));
        assert!(rendered.contains("redirect_path"));
    }

    #[test]
    fn rejects_frontend_ignis_login() {
        let service = ServiceManifest {
            name: "web".to_owned(),
            kind: ServiceKind::Frontend,
            path: PathBuf::from("services/web"),
            prefix: "/".to_owned(),
            http: None,
            frontend: Some(FrontendServiceConfig {
                build_command: vec!["pnpm".to_owned(), "build".to_owned()],
                output_dir: PathBuf::from("dist"),
                spa_fallback: true,
            }),
            ignis_login: Some(IgnisLoginConfig {
                display_name: "Video GIF Studio".to_owned(),
                redirect_path: "/auth/common/callback".to_owned(),
                providers: vec![IgnisLoginProvider::Google],
            }),
            env: BTreeMap::new(),
            secrets: BTreeMap::new(),
            sqlite: SqliteConfig::default(),
            resources: ResourceConfig::default(),
        };

        assert!(service.validate().is_err());
    }

    #[test]
    fn rejects_ignis_login_reserved_secret_conflict() {
        let service = ServiceManifest {
            name: "api".to_owned(),
            kind: ServiceKind::Http,
            path: PathBuf::from("services/api"),
            prefix: "/api".to_owned(),
            http: Some(HttpServiceConfig {
                component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                base_path: "/".to_owned(),
            }),
            frontend: None,
            ignis_login: Some(IgnisLoginConfig {
                display_name: "Video GIF Studio".to_owned(),
                redirect_path: "/auth/common/callback".to_owned(),
                providers: vec![IgnisLoginProvider::Google],
            }),
            env: BTreeMap::new(),
            secrets: BTreeMap::from([(
                String::from(IGNIS_LOGIN_CLIENT_ID_SECRET),
                String::from("manual-client-id"),
            )]),
            sqlite: SqliteConfig::default(),
            resources: ResourceConfig::default(),
        };

        assert!(service.validate().is_err());
    }

    #[test]
    fn rejects_ignis_login_reserved_env_conflict() {
        let service = ServiceManifest {
            name: "api".to_owned(),
            kind: ServiceKind::Http,
            path: PathBuf::from("services/api"),
            prefix: "/api".to_owned(),
            http: Some(HttpServiceConfig {
                component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                base_path: "/".to_owned(),
            }),
            frontend: None,
            ignis_login: Some(IgnisLoginConfig {
                display_name: "Video GIF Studio".to_owned(),
                redirect_path: "/auth/common/callback".to_owned(),
                providers: vec![IgnisLoginProvider::Google],
            }),
            env: BTreeMap::from([(
                String::from(IGNIS_LOGIN_IGNISCLOUD_ID_BASE_URL_ENV),
                String::from("https://id.igniscloud.dev"),
            )]),
            secrets: BTreeMap::new(),
            sqlite: SqliteConfig::default(),
            resources: ResourceConfig::default(),
        };

        assert!(service.validate().is_err());
    }

    #[test]
    fn rejects_empty_ignis_login_provider_list() {
        let service = ServiceManifest {
            name: "api".to_owned(),
            kind: ServiceKind::Http,
            path: PathBuf::from("services/api"),
            prefix: "/api".to_owned(),
            http: Some(HttpServiceConfig {
                component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                base_path: "/".to_owned(),
            }),
            frontend: None,
            ignis_login: Some(IgnisLoginConfig {
                display_name: "Video GIF Studio".to_owned(),
                redirect_path: "/auth/common/callback".to_owned(),
                providers: Vec::new(),
            }),
            env: BTreeMap::new(),
            secrets: BTreeMap::new(),
            sqlite: SqliteConfig::default(),
            resources: ResourceConfig::default(),
        };

        assert!(service.validate().is_err());
    }

    #[test]
    fn rejects_duplicate_ignis_login_providers() {
        let service = ServiceManifest {
            name: "api".to_owned(),
            kind: ServiceKind::Http,
            path: PathBuf::from("services/api"),
            prefix: "/api".to_owned(),
            http: Some(HttpServiceConfig {
                component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                base_path: "/".to_owned(),
            }),
            frontend: None,
            ignis_login: Some(IgnisLoginConfig {
                display_name: "Video GIF Studio".to_owned(),
                redirect_path: "/auth/common/callback".to_owned(),
                providers: vec![IgnisLoginProvider::Google, IgnisLoginProvider::Google],
            }),
            env: BTreeMap::new(),
            secrets: BTreeMap::new(),
            sqlite: SqliteConfig::default(),
            resources: ResourceConfig::default(),
        };

        assert!(service.validate().is_err());
    }
}
