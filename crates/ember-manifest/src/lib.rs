//! Manifest types and helpers for Ember workers.
//!
//! This crate provides:
//! - `worker.toml` parsing and rendering
//! - manifest validation
//! - component signing and verification helpers

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

pub const MANIFEST_FILE: &str = "worker.toml";

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
    #[serde(default)]
    pub network: NetworkConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkConfig {
    #[serde(default)]
    pub mode: NetworkMode,
    #[serde(default)]
    pub allow: Vec<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mode: NetworkMode::DenyAll,
            allow: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    #[default]
    DenyAll,
    AllowList,
    AllowAll,
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

fn default_base_path() -> String {
    "/".to_owned()
}

impl WorkerManifest {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("manifest field `name` cannot be empty");
        }
        if !self
            .name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            bail!("manifest field `name` must contain only letters, numbers, '-' or '_'");
        }
        if self.base_path.is_empty() || !self.base_path.starts_with('/') {
            bail!("manifest field `base_path` must start with '/'");
        }
        if self.component.as_os_str().is_empty() {
            bail!("manifest field `component` cannot be empty");
        }
        validate_binding_names(self.env.keys(), "env")?;
        validate_binding_names(self.secrets.keys(), "secrets")?;
        self.resources.validate()?;
        self.network.validate()?;
        Ok(())
    }

    pub fn render(&self) -> Result<String> {
        toml::to_string_pretty(self).context("rendering worker.toml")
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

impl NetworkConfig {
    pub fn validate(&self) -> Result<()> {
        match self.mode {
            NetworkMode::DenyAll | NetworkMode::AllowAll => {
                if !self.allow.is_empty() {
                    bail!(
                        "manifest field `network.allow` may only be set when `network.mode = \"allow_list\"`"
                    );
                }
            }
            NetworkMode::AllowList => {
                if self.allow.is_empty() {
                    bail!(
                        "manifest field `network.allow` cannot be empty when `network.mode = \"allow_list\"`"
                    );
                }
                for entry in &self.allow {
                    validate_network_allow_entry(entry)?;
                }
            }
        }
        Ok(())
    }

    pub fn allows_authority(&self, authority: &str, host: Option<&str>) -> bool {
        match self.mode {
            NetworkMode::AllowAll => true,
            NetworkMode::DenyAll => false,
            NetworkMode::AllowList => {
                let authority = authority.trim().to_ascii_lowercase();
                let host = host
                    .unwrap_or(authority.as_str())
                    .trim()
                    .to_ascii_lowercase();
                self.allow.iter().any(|rule| {
                    authority_matches_rule(&authority, &host, &rule.to_ascii_lowercase())
                })
            }
        }
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

fn validate_network_allow_entry(entry: &str) -> Result<()> {
    let trimmed = entry.trim();
    if trimmed.is_empty() {
        bail!("manifest field `network.allow` cannot contain empty entries");
    }
    let valid = trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | ':' | '[' | ']'));
    if !valid {
        bail!(
            "manifest field `network.allow` contains invalid entry `{trimmed}`; use host, host:port, .suffix or [ipv6]:port"
        );
    }
    Ok(())
}

fn authority_matches_rule(authority: &str, host: &str, rule: &str) -> bool {
    if rule == authority || rule == host {
        return true;
    }
    if let Some(suffix) = rule.strip_prefix('.') {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }
    false
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
            network: NetworkConfig {
                mode: NetworkMode::AllowList,
                allow: vec!["api.example.com:443".to_owned()],
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
            network: NetworkConfig::default(),
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn rejects_invalid_network_policy() {
        let manifest = WorkerManifest {
            name: "hello_worker".to_owned(),
            component: PathBuf::from("app.wasm"),
            base_path: "/".to_owned(),
            env: BTreeMap::new(),
            secrets: BTreeMap::new(),
            sqlite: SqliteConfig::default(),
            resources: ResourceConfig::default(),
            network: NetworkConfig {
                mode: NetworkMode::AllowList,
                allow: Vec::new(),
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
}
