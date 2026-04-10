use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::{
    FrontendServiceConfig, HttpServiceConfig, IgnisLoginConfig, LoadedManifest,
    LoadedProjectManifest, NetworkConfig, ProjectConfig, ProjectManifest, ResourceConfig,
    ServiceKind, ServiceManifest, SqliteConfig, validate_relative_service_path,
    validate_resource_name, validate_service_prefix_like_path,
};

const DEFAULT_LISTENER_NAME: &str = "public";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSpec {
    pub project: ProjectConfig,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub listeners: Vec<ListenerSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exposes: Vec<ExposeSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<ServiceSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListenerSpec {
    pub name: String,
    #[serde(default)]
    pub protocol: ListenerProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExposeSpec {
    pub name: String,
    pub listener: String,
    pub service: String,
    #[serde(default)]
    pub binding: Option<String>,
    #[serde(default = "default_expose_path")]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceSpec {
    pub name: String,
    pub kind: ServiceKind,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<BindingSpec>,
    #[serde(default)]
    pub http: Option<HttpServiceConfig>,
    #[serde(default)]
    pub frontend: Option<FrontendServiceConfig>,
    #[serde(default)]
    pub ignis_login: Option<IgnisLoginConfig>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secrets: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "SqliteConfig::is_default")]
    pub sqlite: SqliteConfig,
    #[serde(default, skip_serializing_if = "ResourceConfig::is_default")]
    pub resources: ResourceConfig,
    #[serde(default, skip_serializing_if = "NetworkConfig::is_default")]
    pub network: NetworkConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BindingSpec {
    pub name: String,
    pub kind: BindingKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ListenerProtocol {
    #[default]
    Http,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BindingKind {
    Http,
    Frontend,
    Grpc,
    Rpc,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedDependencyGraph {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<CompiledServicePlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledProjectPlan {
    pub project: String,
    pub listeners: Vec<ListenerSpec>,
    pub services: Vec<CompiledServicePlan>,
    pub exposures: Vec<CompiledExposurePlan>,
    pub activations: Vec<ServiceActivationPlan>,
    pub manifest: ProjectManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledServicePlan {
    pub name: String,
    pub kind: ServiceKind,
    pub path: PathBuf,
    pub service_identity: String,
    pub binding: BindingSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledExposurePlan {
    pub name: String,
    pub listener: String,
    pub service: String,
    pub binding: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceActivationPlan {
    pub service: String,
    pub service_identity: String,
    pub binding: String,
    pub route_prefix: String,
    pub service_kind: ServiceKind,
}

fn default_expose_path() -> String {
    "/".to_owned()
}

impl ProjectSpec {
    pub fn validate(&self) -> Result<()> {
        validate_resource_name(&self.project.name, "project field `project.name`")?;

        let mut listeners = BTreeSet::new();
        for listener in &self.listeners {
            listener.validate()?;
            if !listeners.insert(listener.name.clone()) {
                bail!("project contains duplicate listener `{}`", listener.name);
            }
        }

        let mut services = BTreeMap::new();
        for service in &self.services {
            service.validate()?;
            if services.insert(service.name.clone(), service).is_some() {
                bail!("project contains duplicate service `{}`", service.name);
            }
        }

        let mut expose_names = BTreeSet::new();
        let mut listener_paths = BTreeMap::<(String, String), String>::new();
        let mut service_exposures = BTreeMap::<String, String>::new();
        for expose in &self.exposes {
            expose.validate()?;
            if !expose_names.insert(expose.name.clone()) {
                bail!("project contains duplicate exposure `{}`", expose.name);
            }
            if !listeners.contains(&expose.listener) {
                bail!(
                    "exposure `{}` references unknown listener `{}`",
                    expose.name,
                    expose.listener
                );
            }
            let Some(service) = services.get(&expose.service) else {
                bail!(
                    "exposure `{}` references unknown service `{}`",
                    expose.name,
                    expose.service
                );
            };
            let normalized_path = normalize_expose_path(&expose.path)?;
            let key = (expose.listener.clone(), normalized_path.clone());
            if let Some(existing) = listener_paths.insert(key, expose.name.clone()) {
                bail!(
                    "listener `{}` path `{}` is declared by both exposure `{existing}` and `{}`",
                    expose.listener,
                    normalized_path,
                    expose.name
                );
            }
            if let Some(existing) =
                service_exposures.insert(expose.service.clone(), expose.name.clone())
            {
                bail!(
                    "service `{}` is exposed by both `{existing}` and `{}`; current runtime only supports one public exposure per service",
                    expose.service,
                    expose.name
                );
            }

            let binding_name = expose
                .binding
                .as_deref()
                .unwrap_or_else(|| service.default_binding_name());
            let binding = service.binding(binding_name)?;
            if binding.kind != service.required_binding_kind() {
                bail!(
                    "exposure `{}` binding `{}` is incompatible with service `{}` kind `{}`",
                    expose.name,
                    binding.name,
                    service.name,
                    service.kind.as_str()
                );
            }
        }

        for service in &self.services {
            if !service_exposures.contains_key(&service.name) {
                bail!(
                    "service `{}` does not declare an exposure; current runtime still requires one public exposure per service",
                    service.name
                );
            }
        }

        Ok(())
    }

    pub fn compile(&self) -> Result<CompiledProjectPlan> {
        self.validate()?;

        let services_by_name = self
            .services
            .iter()
            .map(|service| (service.name.as_str(), service))
            .collect::<BTreeMap<_, _>>();

        let mut compiled_services = Vec::with_capacity(self.services.len());
        let mut compiled_exposures = Vec::with_capacity(self.exposes.len());
        let mut activations = Vec::with_capacity(self.exposes.len());
        let mut manifest_services = Vec::with_capacity(self.services.len());

        for expose in &self.exposes {
            let service = services_by_name
                .get(expose.service.as_str())
                .ok_or_else(|| anyhow!("service `{}` not found during compile", expose.service))?;
            let normalized_path = normalize_expose_path(&expose.path)?;
            let binding_name = expose
                .binding
                .clone()
                .unwrap_or_else(|| service.default_binding_name().to_owned());
            let binding = service.binding(&binding_name)?;
            let service_identity = format!("svc://{}/{}", self.project.name, service.name);

            compiled_exposures.push(CompiledExposurePlan {
                name: expose.name.clone(),
                listener: expose.listener.clone(),
                service: service.name.clone(),
                binding: binding.name.clone(),
                path: normalized_path.clone(),
            });
            activations.push(ServiceActivationPlan {
                service: service.name.clone(),
                service_identity: service_identity.clone(),
                binding: binding.name.clone(),
                route_prefix: normalized_path.clone(),
                service_kind: service.kind,
            });
            compiled_services.push(CompiledServicePlan {
                name: service.name.clone(),
                kind: service.kind,
                path: service.path.clone(),
                service_identity,
                binding,
            });
            manifest_services.push((*service).clone().into_manifest(normalized_path));
        }

        let manifest = ProjectManifest {
            project: self.project.clone(),
            services: manifest_services,
        };
        manifest.validate()?;

        Ok(CompiledProjectPlan {
            project: self.project.name.clone(),
            listeners: self.listeners.clone(),
            services: compiled_services,
            exposures: compiled_exposures,
            activations,
            manifest,
        })
    }

    pub fn render(&self) -> Result<String> {
        hcl::to_string(self).context("rendering ignis.hcl")
    }

    pub fn from_project_manifest(manifest: &ProjectManifest) -> Result<Self> {
        manifest.validate()?;

        let mut services = Vec::with_capacity(manifest.services.len());
        let mut exposes = Vec::with_capacity(manifest.services.len());
        for service in &manifest.services {
            let binding_name = service.default_binding_name().to_owned();
            services.push(ServiceSpec::from_manifest(service, &binding_name));
            exposes.push(ExposeSpec {
                name: service.name.clone(),
                listener: DEFAULT_LISTENER_NAME.to_owned(),
                service: service.name.clone(),
                binding: Some(binding_name),
                path: service.prefix.clone(),
            });
        }

        Ok(Self {
            project: manifest.project.clone(),
            listeners: if manifest.services.is_empty() {
                Vec::new()
            } else {
                vec![ListenerSpec {
                    name: DEFAULT_LISTENER_NAME.to_owned(),
                    protocol: ListenerProtocol::Http,
                }]
            },
            exposes,
            services,
        })
    }
}

impl ListenerSpec {
    fn validate(&self) -> Result<()> {
        validate_resource_name(&self.name, "listener field `name`")
    }
}

impl ExposeSpec {
    fn validate(&self) -> Result<()> {
        validate_resource_name(&self.name, "expose field `name`")?;
        validate_resource_name(&self.listener, "expose field `listener`")?;
        validate_resource_name(&self.service, "expose field `service`")?;
        validate_service_prefix_like_path(&self.path, "expose field `path`")
    }
}

impl ServiceSpec {
    fn validate(&self) -> Result<()> {
        validate_resource_name(&self.name, "service field `name`")?;
        validate_relative_service_path(&self.path)?;
        let expected_kind = self.required_binding_kind();

        if self.bindings.is_empty() {
            let _ = self.synthetic_manifest("/_validate")?;
            return Ok(());
        }

        let mut binding_names = BTreeSet::new();
        for binding in &self.bindings {
            validate_resource_name(&binding.name, "binding field `name`")?;
            if !binding_names.insert(binding.name.clone()) {
                bail!(
                    "service `{}` contains duplicate binding `{}`",
                    self.name,
                    binding.name
                );
            }
            if binding.kind != expected_kind {
                bail!(
                    "service `{}` binding `{}` kind `{}` is not supported for service kind `{}`",
                    self.name,
                    binding.name,
                    binding.kind.as_str(),
                    self.kind.as_str()
                );
            }
        }

        let _ = self.synthetic_manifest("/_validate")?;
        Ok(())
    }

    fn synthetic_manifest(&self, prefix: &str) -> Result<ServiceManifest> {
        let manifest = self.clone().into_manifest(prefix.to_owned());
        manifest.validate()?;
        Ok(manifest)
    }

    fn from_manifest(service: &ServiceManifest, binding_name: &str) -> Self {
        Self {
            name: service.name.clone(),
            kind: service.kind,
            path: service.path.clone(),
            bindings: vec![BindingSpec {
                name: binding_name.to_owned(),
                kind: service.required_binding_kind(),
            }],
            http: service.http.clone(),
            frontend: service.frontend.clone(),
            ignis_login: service.ignis_login.clone(),
            env: service.env.clone(),
            secrets: service.secrets.clone(),
            sqlite: service.sqlite.clone(),
            resources: service.resources.clone(),
            network: service.network.clone(),
        }
    }

    fn into_manifest(self, prefix: String) -> ServiceManifest {
        ServiceManifest {
            name: self.name,
            kind: self.kind,
            path: self.path,
            prefix,
            http: self.http,
            frontend: self.frontend,
            ignis_login: self.ignis_login,
            env: self.env,
            secrets: self.secrets,
            sqlite: self.sqlite,
            resources: self.resources,
            network: self.network,
        }
    }

    fn binding(&self, name: &str) -> Result<BindingSpec> {
        if self.bindings.is_empty() && name == self.default_binding_name() {
            return Ok(BindingSpec {
                name: name.to_owned(),
                kind: self.required_binding_kind(),
            });
        }

        self.bindings
            .iter()
            .find(|binding| binding.name == name)
            .cloned()
            .ok_or_else(|| anyhow!("service `{}` does not define binding `{name}`", self.name))
    }

    fn default_binding_name(&self) -> &'static str {
        match self.kind {
            ServiceKind::Http => "http",
            ServiceKind::Frontend => "frontend",
        }
    }

    fn required_binding_kind(&self) -> BindingKind {
        match self.kind {
            ServiceKind::Http => BindingKind::Http,
            ServiceKind::Frontend => BindingKind::Frontend,
        }
    }
}

impl BindingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Frontend => "frontend",
            Self::Grpc => "grpc",
            Self::Rpc => "rpc",
        }
    }
}

impl ListenerProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
        }
    }
}

impl ServiceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Frontend => "frontend",
        }
    }
}

impl SqliteConfig {
    pub(crate) fn is_default(&self) -> bool {
        !self.enabled
    }
}

impl ResourceConfig {
    pub(crate) fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

impl NetworkConfig {
    pub(crate) fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

impl ServiceManifest {
    fn default_binding_name(&self) -> &'static str {
        match self.kind {
            ServiceKind::Http => "http",
            ServiceKind::Frontend => "frontend",
        }
    }

    fn required_binding_kind(&self) -> BindingKind {
        match self.kind {
            ServiceKind::Http => BindingKind::Http,
            ServiceKind::Frontend => BindingKind::Frontend,
        }
    }
}

impl ProjectManifest {
    pub fn render(&self) -> Result<String> {
        ProjectSpec::from_project_manifest(self)?.render()
    }
}

impl LoadedProjectManifest {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let input = path.as_ref();
        let manifest_path = if input.is_dir() {
            input.join(crate::PROJECT_MANIFEST_FILE)
        } else {
            input.to_path_buf()
        };
        let raw = fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading manifest at {}", manifest_path.display()))?;
        let spec: ProjectSpec = hcl::from_str(&raw)
            .with_context(|| format!("parsing manifest at {}", manifest_path.display()))?;
        let compiled = spec.compile()?;
        let project_dir = manifest_path
            .parent()
            .ok_or_else(|| anyhow!("manifest path has no parent: {}", manifest_path.display()))?
            .to_path_buf();
        Ok(Self {
            manifest_path,
            project_dir,
            spec,
            manifest: compiled.manifest,
        })
    }

    pub fn compiled_plan(&self) -> Result<CompiledProjectPlan> {
        self.spec.compile()
    }

    pub fn project_name(&self) -> &str {
        self.manifest.project.name.trim()
    }

    pub fn find_service(&self, name: &str) -> Option<&ServiceManifest> {
        let name = name.trim();
        self.manifest
            .services
            .iter()
            .find(|service| service.name == name)
    }

    pub fn service_dir(&self, service: &ServiceManifest) -> PathBuf {
        self.project_dir.join(&service.path)
    }

    pub fn http_service_manifest(&self, service_name: &str) -> Result<LoadedManifest> {
        let service = self
            .find_service(service_name)
            .ok_or_else(|| anyhow!("service `{service_name}` not found"))?;
        let manifest = service.http_worker_manifest()?;
        let service_dir = self.service_dir(service);
        LoadedManifest::from_parts(&self.manifest_path, service_dir, manifest)
    }
}

fn normalize_expose_path(path: &str) -> Result<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        bail!("expose field `path` cannot be empty");
    }
    validate_service_prefix_like_path(trimmed, "expose field `path`")?;
    if trimmed != "/" && trimmed.ends_with('/') {
        return Ok(trimmed.trim_end_matches('/').to_owned());
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::IgnisLoginProvider;

    #[test]
    fn compiles_hcl_project_spec_into_internal_manifest() {
        let spec = ProjectSpec {
            project: ProjectConfig {
                name: "demo".to_owned(),
            },
            listeners: vec![ListenerSpec {
                name: "public".to_owned(),
                protocol: ListenerProtocol::Http,
            }],
            exposes: vec![
                ExposeSpec {
                    name: "api".to_owned(),
                    listener: "public".to_owned(),
                    service: "api".to_owned(),
                    binding: Some("http".to_owned()),
                    path: "/api".to_owned(),
                },
                ExposeSpec {
                    name: "web".to_owned(),
                    listener: "public".to_owned(),
                    service: "web".to_owned(),
                    binding: Some("frontend".to_owned()),
                    path: "/".to_owned(),
                },
            ],
            services: vec![
                ServiceSpec {
                    name: "api".to_owned(),
                    kind: ServiceKind::Http,
                    path: PathBuf::from("services/api"),
                    bindings: vec![BindingSpec {
                        name: "http".to_owned(),
                        kind: BindingKind::Http,
                    }],
                    http: Some(HttpServiceConfig {
                        component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                        base_path: "/".to_owned(),
                    }),
                    frontend: None,
                    ignis_login: Some(IgnisLoginConfig {
                        display_name: "demo".to_owned(),
                        redirect_path: "/auth/callback".to_owned(),
                        providers: vec![IgnisLoginProvider::Google],
                    }),
                    env: BTreeMap::new(),
                    secrets: BTreeMap::new(),
                    sqlite: SqliteConfig::default(),
                    resources: ResourceConfig::default(),
                    network: NetworkConfig::default(),
                },
                ServiceSpec {
                    name: "web".to_owned(),
                    kind: ServiceKind::Frontend,
                    path: PathBuf::from("services/web"),
                    bindings: vec![BindingSpec {
                        name: "frontend".to_owned(),
                        kind: BindingKind::Frontend,
                    }],
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
                    network: NetworkConfig::default(),
                },
            ],
        };

        let compiled = spec.compile().unwrap();
        assert_eq!(compiled.manifest.services.len(), 2);
        assert_eq!(compiled.manifest.services[0].prefix, "/api");
        assert_eq!(compiled.manifest.services[1].prefix, "/");
        assert_eq!(compiled.activations[0].service_identity, "svc://demo/api");
    }

    #[test]
    fn rejects_service_without_exposure() {
        let spec = ProjectSpec {
            project: ProjectConfig {
                name: "demo".to_owned(),
            },
            listeners: vec![ListenerSpec {
                name: "public".to_owned(),
                protocol: ListenerProtocol::Http,
            }],
            exposes: Vec::new(),
            services: vec![ServiceSpec {
                name: "api".to_owned(),
                kind: ServiceKind::Http,
                path: PathBuf::from("services/api"),
                bindings: vec![BindingSpec {
                    name: "http".to_owned(),
                    kind: BindingKind::Http,
                }],
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
                network: NetworkConfig::default(),
            }],
        };

        assert!(spec.validate().is_err());
    }

    #[test]
    fn loads_workspace_hcl_examples() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");

        for example in [
            "examples/hello-fullstack/ignis.hcl",
            "examples/sqlite-example/ignis.hcl",
            "examples/ignis-login-example/ignis.hcl",
        ] {
            let path = repo_root.join(example);
            let loaded = LoadedProjectManifest::load(&path)
                .unwrap_or_else(|error| panic!("failed to load {}: {error:#}", path.display()));
            assert!(!loaded.manifest.services.is_empty(), "{}", path.display());
        }
    }
}
