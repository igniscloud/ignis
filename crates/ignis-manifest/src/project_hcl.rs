use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::{
    BUILTIN_AGENT_SERVICE_IMAGE, BUILTIN_OPENCODE_AGENT_SERVICE_IMAGE, FrontendServiceConfig,
    HttpServiceConfig, INTERNAL_ONLY_MANIFEST_PREFIX_BASE, IgnisLoginConfig, JobSpec,
    LoadedManifest, LoadedProjectManifest, ProjectAutomationConfig, ProjectConfig, ProjectManifest,
    ResourceConfig, ScheduleSpec, ServiceKind, ServiceManifest, SqliteConfig,
    validate_relative_service_path, validate_resource_name, validate_service_prefix_like_path,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<JobSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schedules: Vec<ScheduleSpec>,
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
    #[serde(default, skip_serializing_if = "AgentRuntime::is_default")]
    pub agent_runtime: AgentRuntime,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<BindingSpec>,
    #[serde(default)]
    pub http: Option<HttpServiceConfig>,
    #[serde(default)]
    pub frontend: Option<FrontendServiceConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentServiceConfig>,
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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntime {
    #[default]
    Codex,
    Opencode,
}

impl AgentRuntime {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Opencode => "opencode",
        }
    }

    pub(crate) fn is_default(&self) -> bool {
        *self == Self::Codex
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentServiceConfig {
    pub image: String,
    #[serde(default = "default_agent_port")]
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workdir: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

fn default_agent_port() -> u16 {
    8080
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
    pub bindings: Vec<CompiledBindingPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublishedServicePlan {
    pub name: String,
    pub kind: ServiceKind,
    pub service_identity: String,
    pub bindings: Vec<PublishedBindingPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublishedBindingPlan {
    pub name: String,
    pub binding_identity: String,
    pub protocol: BindingKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_exposures: Vec<PublishedExposurePlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublishedExposurePlan {
    pub name: String,
    pub listener: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledBindingPlan {
    pub name: String,
    pub binding_identity: String,
    pub protocol: BindingKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_exposures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledExposurePlan {
    pub name: String,
    pub listener: String,
    pub service: String,
    pub service_identity: String,
    pub binding: String,
    pub binding_identity: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceActivationPlan {
    pub service: String,
    pub service_identity: String,
    pub binding: String,
    pub binding_identity: String,
    pub protocol: BindingKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_exposures: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_prefix: Option<String>,
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

        let automation_services = self
            .services
            .iter()
            .map(|service| service.synthetic_manifest("/_validate_automation"))
            .collect::<Result<Vec<_>>>()?;
        ProjectAutomationConfig {
            jobs: self.jobs.clone(),
            schedules: self.schedules.clone(),
        }
        .validate_against_services(&automation_services)?;

        let mut expose_names = BTreeSet::new();
        let mut listener_paths = BTreeMap::<(String, String), String>::new();
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

            let binding_name = expose
                .binding
                .as_deref()
                .unwrap_or_else(|| service.default_binding_name());
            let binding = service.binding(binding_name)?;
            if binding.kind != service.public_exposure_binding_kind() {
                bail!(
                    "exposure `{}` binding `{}` is incompatible with service `{}` kind `{}`",
                    expose.name,
                    binding.name,
                    service.name,
                    service.kind.as_str()
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

        let mut compiled_exposures = Vec::with_capacity(self.exposes.len());
        let mut exposures_by_binding =
            BTreeMap::<(String, String), Vec<CompiledExposurePlan>>::new();
        let mut manifest_prefixes = BTreeMap::<String, String>::new();

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
            let binding_identity = format!("{service_identity}#{}", binding.name);

            let compiled_exposure = CompiledExposurePlan {
                name: expose.name.clone(),
                listener: expose.listener.clone(),
                service: service.name.clone(),
                service_identity: service_identity.clone(),
                binding: binding.name.clone(),
                binding_identity: binding_identity.clone(),
                path: normalized_path.clone(),
            };
            compiled_exposures.push(compiled_exposure.clone());
            exposures_by_binding
                .entry((service.name.clone(), binding.name.clone()))
                .or_default()
                .push(compiled_exposure);
            manifest_prefixes
                .entry(service.name.clone())
                .or_insert(normalized_path);
        }

        let mut manifest_services = Vec::with_capacity(self.services.len());
        let mut compiled_services = Vec::with_capacity(self.services.len());
        let mut activations = Vec::new();
        for service in &self.services {
            let service_identity = format!("svc://{}/{}", self.project.name, service.name);
            let manifest_prefix = manifest_prefixes
                .get(&service.name)
                .cloned()
                .unwrap_or_else(|| internal_only_manifest_prefix(&service.name));
            manifest_services.push(service.clone().into_manifest(manifest_prefix));
            let mut compiled_bindings = Vec::new();
            for binding in service.bindings_for_compile() {
                let binding_identity = format!("{service_identity}#{}", binding.name);
                let public_exposures = exposures_by_binding
                    .get(&(service.name.clone(), binding.name.clone()))
                    .cloned()
                    .unwrap_or_default();
                let public_exposure_names = public_exposures
                    .iter()
                    .map(|exposure| exposure.name.clone())
                    .collect::<Vec<_>>();
                let route_prefix = public_exposures
                    .first()
                    .map(|exposure| exposure.path.clone());
                compiled_bindings.push(CompiledBindingPlan {
                    name: binding.name.clone(),
                    binding_identity: binding_identity.clone(),
                    protocol: binding.kind,
                    public_exposures: public_exposure_names.clone(),
                });
                activations.push(ServiceActivationPlan {
                    service: service.name.clone(),
                    service_identity: service_identity.clone(),
                    binding: binding.name.clone(),
                    binding_identity,
                    protocol: binding.kind,
                    public_exposures: public_exposure_names,
                    route_prefix,
                    service_kind: service.kind,
                });
            }
            compiled_services.push(CompiledServicePlan {
                name: service.name.clone(),
                kind: service.kind,
                path: service.path.clone(),
                service_identity,
                bindings: compiled_bindings,
            });
        }

        let manifest = ProjectManifest {
            project: self.project.clone(),
            services: manifest_services,
            jobs: self.jobs.clone(),
            schedules: self.schedules.clone(),
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
        let mut exposes = Vec::new();
        for service in &manifest.services {
            let binding_name = service.default_binding_name().to_owned();
            services.push(ServiceSpec::from_manifest(service, &binding_name));
            if !is_internal_only_manifest_prefix(&service.prefix) {
                exposes.push(ExposeSpec {
                    name: service.name.clone(),
                    listener: DEFAULT_LISTENER_NAME.to_owned(),
                    service: service.name.clone(),
                    binding: Some(binding_name),
                    path: service.prefix.clone(),
                });
            }
        }

        Ok(Self {
            project: manifest.project.clone(),
            listeners: if exposes.is_empty() {
                Vec::new()
            } else {
                vec![ListenerSpec {
                    name: DEFAULT_LISTENER_NAME.to_owned(),
                    protocol: ListenerProtocol::Http,
                }]
            },
            exposes,
            services,
            jobs: manifest.jobs.clone(),
            schedules: manifest.schedules.clone(),
        })
    }
}

impl CompiledProjectPlan {
    pub fn published_service_plan(&self, service_name: &str) -> Result<PublishedServicePlan> {
        let service = self
            .services
            .iter()
            .find(|service| service.name == service_name)
            .ok_or_else(|| anyhow!("compiled plan does not contain service `{service_name}`"))?;
        let bindings = service
            .bindings
            .iter()
            .map(|binding| PublishedBindingPlan {
                name: binding.name.clone(),
                binding_identity: binding.binding_identity.clone(),
                protocol: binding.protocol,
                public_exposures: self
                    .exposures
                    .iter()
                    .filter(|exposure| {
                        exposure.service == service.name && exposure.binding == binding.name
                    })
                    .map(|exposure| PublishedExposurePlan {
                        name: exposure.name.clone(),
                        listener: exposure.listener.clone(),
                        path: exposure.path.clone(),
                    })
                    .collect(),
            })
            .collect();

        Ok(PublishedServicePlan {
            name: service.name.clone(),
            kind: service.kind,
            service_identity: service.service_identity.clone(),
            bindings,
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
            if !self.supports_binding_kind(binding.kind) {
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
            agent_runtime: service.agent_runtime,
            bindings: vec![BindingSpec {
                name: binding_name.to_owned(),
                kind: service.public_exposure_binding_kind(),
            }],
            http: service.http.clone(),
            frontend: service.frontend.clone(),
            agent: service.agent.clone(),
            ignis_login: service.ignis_login.clone(),
            env: service.env.clone(),
            secrets: service.secrets.clone(),
            sqlite: service.sqlite.clone(),
            resources: service.resources.clone(),
        }
    }

    fn into_manifest(self, prefix: String) -> ServiceManifest {
        ServiceManifest {
            name: self.name,
            kind: self.kind,
            path: self.path,
            prefix,
            agent_runtime: self.agent_runtime,
            http: self.http,
            frontend: self.frontend,
            agent: self.agent,
            ignis_login: self.ignis_login,
            env: self.env,
            secrets: self.secrets,
            sqlite: self.sqlite,
            resources: self.resources,
        }
    }

    fn binding(&self, name: &str) -> Result<BindingSpec> {
        if self.bindings.is_empty() && name == self.default_binding_name() {
            return Ok(BindingSpec {
                name: name.to_owned(),
                kind: self.public_exposure_binding_kind(),
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
            ServiceKind::Agent => "http",
        }
    }

    fn public_exposure_binding_kind(&self) -> BindingKind {
        match self.kind {
            ServiceKind::Http => BindingKind::Http,
            ServiceKind::Frontend => BindingKind::Frontend,
            ServiceKind::Agent => BindingKind::Http,
        }
    }

    fn supports_binding_kind(&self, kind: BindingKind) -> bool {
        match self.kind {
            ServiceKind::Http => matches!(kind, BindingKind::Http),
            ServiceKind::Frontend => matches!(kind, BindingKind::Frontend),
            ServiceKind::Agent => matches!(kind, BindingKind::Http),
        }
    }

    fn bindings_for_compile(&self) -> Vec<BindingSpec> {
        if self.bindings.is_empty() {
            vec![BindingSpec {
                name: self.default_binding_name().to_owned(),
                kind: self.public_exposure_binding_kind(),
            }]
        } else {
            self.bindings.clone()
        }
    }
}

impl BindingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Frontend => "frontend",
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
            Self::Agent => "agent",
        }
    }
}

impl AgentServiceConfig {
    pub fn validate(&self, service_name: &str, runtime: AgentRuntime) -> Result<()> {
        if self.image.trim().is_empty() {
            bail!("agent service `{service_name}` field `agent.image` cannot be empty");
        }
        if self.image.contains(char::is_whitespace) {
            bail!("agent service `{service_name}` field `agent.image` cannot contain whitespace");
        }
        let expected_image = match runtime {
            AgentRuntime::Codex => BUILTIN_AGENT_SERVICE_IMAGE,
            AgentRuntime::Opencode => BUILTIN_OPENCODE_AGENT_SERVICE_IMAGE,
        };
        if self.image != expected_image {
            bail!(
                "agent service `{service_name}` field `agent.image` must be `{expected_image}` for `{}` runtime; custom agent images are not supported yet",
                runtime.as_str()
            );
        }
        if self.port == 0 {
            bail!("agent service `{service_name}` field `agent.port` must be greater than 0");
        }
        if let Some(framework) = self.framework.as_deref() {
            validate_agent_token(
                framework,
                &format!("agent service `{service_name}` field `agent.framework`"),
            )?;
            if framework != runtime.as_str() {
                bail!(
                    "agent service `{service_name}` field `agent.framework` must be `{}` for `{}` runtime",
                    runtime.as_str(),
                    runtime.as_str()
                );
            }
        }
        if let Some(workdir) = self.workdir.as_deref() {
            if workdir.trim().is_empty() {
                bail!("agent service `{service_name}` field `agent.workdir` cannot be empty");
            }
        }
        for (index, item) in self.command.iter().enumerate() {
            if item.trim().is_empty() {
                bail!(
                    "agent service `{service_name}` field `agent.command[{index}]` cannot be empty"
                );
            }
        }
        for (index, item) in self.args.iter().enumerate() {
            if item.trim().is_empty() {
                bail!("agent service `{service_name}` field `agent.args[{index}]` cannot be empty");
            }
        }
        Ok(())
    }
}

fn validate_agent_token(value: &str, field_name: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{field_name} cannot be empty");
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
    {
        bail!("{field_name} must contain only letters, numbers, '-', '_' or '.'");
    }
    Ok(())
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

impl ServiceManifest {
    fn default_binding_name(&self) -> &'static str {
        match self.kind {
            ServiceKind::Http => "http",
            ServiceKind::Frontend => "frontend",
            ServiceKind::Agent => "http",
        }
    }

    fn public_exposure_binding_kind(&self) -> BindingKind {
        match self.kind {
            ServiceKind::Http => BindingKind::Http,
            ServiceKind::Frontend => BindingKind::Frontend,
            ServiceKind::Agent => BindingKind::Http,
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
            compiled_plan: compiled.clone(),
            manifest: compiled.manifest,
        })
    }

    pub fn compiled_plan(&self) -> Result<CompiledProjectPlan> {
        Ok(self.compiled_plan.clone())
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

fn internal_only_manifest_prefix(service_name: &str) -> String {
    format!(
        "{}/{}",
        INTERNAL_ONLY_MANIFEST_PREFIX_BASE,
        service_name.trim().trim_matches('/')
    )
}

fn is_internal_only_manifest_prefix(prefix: &str) -> bool {
    prefix == INTERNAL_ONLY_MANIFEST_PREFIX_BASE
        || prefix
            .strip_prefix(INTERNAL_ONLY_MANIFEST_PREFIX_BASE)
            .map(|suffix| suffix.starts_with('/'))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::{
        IgnisLoginProvider, JobConcurrencySpec, JobRetentionSpec, JobRetrySpec, JobTargetSpec,
    };

    #[test]
    fn compiles_hcl_project_spec_into_internal_manifest() {
        let spec = ProjectSpec {
            project: ProjectConfig {
                name: "demo".to_owned(),
                domain: None,
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
                    agent_runtime: AgentRuntime::Codex,
                    bindings: vec![BindingSpec {
                        name: "http".to_owned(),
                        kind: BindingKind::Http,
                    }],
                    http: Some(HttpServiceConfig {
                        component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                        base_path: "/".to_owned(),
                    }),
                    frontend: None,
                    agent: None,
                    ignis_login: Some(IgnisLoginConfig {
                        display_name: "demo".to_owned(),
                        redirect_path: "/auth/callback".to_owned(),
                        providers: vec![IgnisLoginProvider::Google],
                    }),
                    env: BTreeMap::new(),
                    secrets: BTreeMap::new(),
                    sqlite: SqliteConfig::default(),
                    resources: ResourceConfig::default(),
                },
                ServiceSpec {
                    name: "web".to_owned(),
                    kind: ServiceKind::Frontend,
                    path: PathBuf::from("services/web"),
                    agent_runtime: AgentRuntime::Codex,
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
                    agent: None,
                    ignis_login: None,
                    env: BTreeMap::new(),
                    secrets: BTreeMap::new(),
                    sqlite: SqliteConfig::default(),
                    resources: ResourceConfig::default(),
                },
            ],
            jobs: Vec::new(),
            schedules: Vec::new(),
        };

        let compiled = spec.compile().unwrap();
        assert_eq!(compiled.manifest.services.len(), 2);
        assert_eq!(compiled.manifest.services[0].prefix, "/api");
        assert_eq!(compiled.manifest.services[1].prefix, "/");
        assert_eq!(compiled.services.len(), 2);
        assert_eq!(compiled.services[0].bindings.len(), 1);
        assert_eq!(
            compiled.services[0].bindings[0].binding_identity,
            "svc://demo/api#http"
        );
        assert_eq!(compiled.activations[0].service_identity, "svc://demo/api");
        assert_eq!(
            compiled.activations[0].route_prefix.as_deref(),
            Some("/api")
        );
    }

    #[test]
    fn compiles_agent_service_with_http_binding_and_job_target() {
        let spec = ProjectSpec {
            project: ProjectConfig {
                name: "demo".to_owned(),
                domain: None,
            },
            listeners: vec![ListenerSpec {
                name: "public".to_owned(),
                protocol: ListenerProtocol::Http,
            }],
            exposes: vec![ExposeSpec {
                name: "coder".to_owned(),
                listener: "public".to_owned(),
                service: "coder".to_owned(),
                binding: None,
                path: "/coder".to_owned(),
            }],
            services: vec![ServiceSpec {
                name: "coder".to_owned(),
                kind: ServiceKind::Agent,
                path: PathBuf::from("services/coder"),
                agent_runtime: AgentRuntime::Codex,
                bindings: Vec::new(),
                http: None,
                frontend: None,
                agent: Some(AgentServiceConfig {
                    image: BUILTIN_AGENT_SERVICE_IMAGE.to_owned(),
                    port: 3900,
                    framework: Some("codex".to_owned()),
                    workdir: Some("/app/work".to_owned()),
                    command: Vec::new(),
                    args: Vec::new(),
                }),
                ignis_login: None,
                env: BTreeMap::new(),
                secrets: BTreeMap::from([(
                    "OPENAI_API_KEY".to_owned(),
                    "secret://openai".to_owned(),
                )]),
                sqlite: SqliteConfig::default(),
                resources: ResourceConfig {
                    memory_limit_bytes: Some(1024 * 1024 * 1024),
                },
            }],
            jobs: vec![JobSpec {
                name: "run-agent".to_owned(),
                queue: "default".to_owned(),
                target: JobTargetSpec {
                    service: "coder".to_owned(),
                    binding: None,
                    path: "/runs".to_owned(),
                    method: "POST".to_owned(),
                },
                timeout_ms: Some(60_000),
                retry: JobRetrySpec {
                    max_attempts: 1,
                    ..JobRetrySpec::default()
                },
                concurrency: JobConcurrencySpec::default(),
                retention: JobRetentionSpec::default(),
            }],
            schedules: Vec::new(),
        };

        let compiled = spec.compile().unwrap();
        let service = compiled
            .services
            .iter()
            .find(|service| service.name == "coder")
            .unwrap();
        assert_eq!(service.kind, ServiceKind::Agent);
        assert_eq!(service.bindings[0].name, "http");
        assert_eq!(service.bindings[0].protocol, BindingKind::Http);
        assert_eq!(compiled.manifest.services[0].prefix, "/coder");
    }

    #[test]
    fn rejects_custom_agent_image() {
        let agent = AgentServiceConfig {
            image: "registry.example.com/custom-agent:latest".to_owned(),
            port: 3900,
            framework: Some("codex".to_owned()),
            workdir: Some("/app/work".to_owned()),
            command: Vec::new(),
            args: Vec::new(),
        };

        let error = agent
            .validate("coder", AgentRuntime::Codex)
            .unwrap_err()
            .to_string();

        assert!(error.contains("custom agent images are not supported yet"));
    }

    #[test]
    fn validates_opencode_builtin_agent_image() {
        let agent = crate::builtin_opencode_agent_service_config();

        agent
            .validate("opencode-agent-service", AgentRuntime::Opencode)
            .unwrap();

        let error = agent
            .validate("opencode-agent-service", AgentRuntime::Codex)
            .unwrap_err()
            .to_string();
        assert!(error.contains("custom agent images are not supported yet"));
    }

    #[test]
    fn renders_agent_service_without_internal_runtime_details() {
        let manifest = ProjectManifest {
            project: ProjectConfig {
                name: "demo".to_owned(),
                domain: None,
            },
            services: vec![ServiceManifest {
                name: "agent-service".to_owned(),
                kind: ServiceKind::Agent,
                path: PathBuf::from("services/agent-service"),
                prefix: "/_ignis_internal/agent-service".to_owned(),
                agent_runtime: AgentRuntime::Codex,
                http: None,
                frontend: None,
                agent: None,
                ignis_login: None,
                env: BTreeMap::new(),
                secrets: BTreeMap::new(),
                sqlite: SqliteConfig::default(),
                resources: ResourceConfig {
                    memory_limit_bytes: Some(1024 * 1024 * 1024),
                },
            }],
            jobs: Vec::new(),
            schedules: Vec::new(),
        };

        let rendered = manifest.render().unwrap();

        assert!(
            rendered.contains("kind = agent")
                || rendered.contains("kind = \"agent\"")
                || rendered.contains("\"kind\" = \"agent\"")
        );
        assert!(!rendered.contains("exposes ="));
        assert!(!rendered.contains("agent ="));
        assert!(!rendered.contains("AGENT_SERVICE_"));
    }

    #[test]
    fn renders_opencode_agent_runtime_without_internal_config() {
        let manifest = ProjectManifest {
            project: ProjectConfig {
                name: "demo".to_owned(),
                domain: None,
            },
            services: vec![ServiceManifest {
                name: "opencode-agent-service".to_owned(),
                kind: ServiceKind::Agent,
                path: PathBuf::from("services/opencode-agent-service"),
                prefix: "/_ignis_internal/opencode-agent-service".to_owned(),
                agent_runtime: AgentRuntime::Opencode,
                http: None,
                frontend: None,
                agent: None,
                ignis_login: None,
                env: BTreeMap::new(),
                secrets: BTreeMap::new(),
                sqlite: SqliteConfig::default(),
                resources: ResourceConfig {
                    memory_limit_bytes: Some(1024 * 1024 * 1024),
                },
            }],
            jobs: Vec::new(),
            schedules: Vec::new(),
        };

        let rendered = manifest.render().unwrap();

        assert!(rendered.contains("agent_runtime"));
        assert!(rendered.contains("opencode"));
        assert!(!rendered.contains("agent ="));
        assert!(!rendered.contains("AGENT_SERVICE_"));
    }

    #[test]
    fn allows_service_without_public_exposure() {
        let spec = ProjectSpec {
            project: ProjectConfig {
                name: "demo".to_owned(),
                domain: None,
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
                agent_runtime: AgentRuntime::Codex,
                bindings: vec![BindingSpec {
                    name: "http".to_owned(),
                    kind: BindingKind::Http,
                }],
                http: Some(HttpServiceConfig {
                    component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                    base_path: "/".to_owned(),
                }),
                frontend: None,
                agent: None,
                ignis_login: None,
                env: BTreeMap::new(),
                secrets: BTreeMap::new(),
                sqlite: SqliteConfig::default(),
                resources: ResourceConfig::default(),
            }],
            jobs: Vec::new(),
            schedules: Vec::new(),
        };

        let compiled = spec.compile().unwrap();
        assert_eq!(compiled.services.len(), 1);
        assert!(compiled.exposures.is_empty());
        assert_eq!(compiled.manifest.services.len(), 1);
        assert_eq!(compiled.manifest.services[0].prefix, "/_ignis_internal/api");
    }

    #[test]
    fn builds_published_service_plan_with_binding_exposure_details() {
        let spec = ProjectSpec {
            project: ProjectConfig {
                name: "demo".to_owned(),
                domain: None,
            },
            listeners: vec![ListenerSpec {
                name: "public".to_owned(),
                protocol: ListenerProtocol::Http,
            }],
            exposes: vec![ExposeSpec {
                name: "api".to_owned(),
                listener: "public".to_owned(),
                service: "api".to_owned(),
                binding: Some("http".to_owned()),
                path: "/api".to_owned(),
            }],
            services: vec![ServiceSpec {
                name: "api".to_owned(),
                kind: ServiceKind::Http,
                path: PathBuf::from("services/api"),
                agent_runtime: AgentRuntime::Codex,
                bindings: vec![BindingSpec {
                    name: "http".to_owned(),
                    kind: BindingKind::Http,
                }],
                http: Some(HttpServiceConfig {
                    component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                    base_path: "/".to_owned(),
                }),
                frontend: None,
                agent: None,
                ignis_login: None,
                env: BTreeMap::new(),
                secrets: BTreeMap::new(),
                sqlite: SqliteConfig::default(),
                resources: ResourceConfig::default(),
            }],
            jobs: Vec::new(),
            schedules: Vec::new(),
        };

        let compiled = spec.compile().unwrap();
        let published = compiled.published_service_plan("api").unwrap();

        assert_eq!(published.service_identity, "svc://demo/api");
        assert_eq!(published.bindings.len(), 1);
        assert_eq!(published.bindings[0].name, "http");
        assert_eq!(published.bindings[0].public_exposures.len(), 1);
        assert_eq!(published.bindings[0].public_exposures[0].listener, "public");
        assert_eq!(published.bindings[0].public_exposures[0].path, "/api");
    }

    #[test]
    fn allows_multiple_public_exposures_for_same_service_binding() {
        let spec = ProjectSpec {
            project: ProjectConfig {
                name: "demo".to_owned(),
                domain: None,
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
                    name: "api-v2".to_owned(),
                    listener: "public".to_owned(),
                    service: "api".to_owned(),
                    binding: Some("http".to_owned()),
                    path: "/v2/api".to_owned(),
                },
            ],
            services: vec![ServiceSpec {
                name: "api".to_owned(),
                kind: ServiceKind::Http,
                path: PathBuf::from("services/api"),
                agent_runtime: AgentRuntime::Codex,
                bindings: vec![BindingSpec {
                    name: "http".to_owned(),
                    kind: BindingKind::Http,
                }],
                http: Some(HttpServiceConfig {
                    component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                    base_path: "/".to_owned(),
                }),
                frontend: None,
                agent: None,
                ignis_login: None,
                env: BTreeMap::new(),
                secrets: BTreeMap::new(),
                sqlite: SqliteConfig::default(),
                resources: ResourceConfig::default(),
            }],
            jobs: Vec::new(),
            schedules: Vec::new(),
        };

        let compiled = spec.compile().unwrap();
        assert_eq!(compiled.exposures.len(), 2);
        assert_eq!(compiled.services.len(), 1);
        assert_eq!(compiled.services[0].bindings.len(), 1);
        assert_eq!(
            compiled.services[0].bindings[0].public_exposures,
            vec!["api".to_owned(), "api-v2".to_owned()]
        );
        assert_eq!(
            compiled.activations[0].route_prefix.as_deref(),
            Some("/api")
        );
        let published = compiled.published_service_plan("api").unwrap();
        assert_eq!(published.bindings[0].public_exposures.len(), 2);
        assert_eq!(published.bindings[0].public_exposures[1].path, "/v2/api");
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

    #[test]
    fn ignores_legacy_cpu_time_limit_in_project_hcl() {
        let spec: ProjectSpec = hcl::from_str(
            r#"
project = {
  name = "legacy"
}

services = [
  {
    name = "api"
    kind = "http"
    path = "services/api"
    http = {
      component = "target/wasm32-wasip2/release/api.wasm"
      base_path = "/"
    }
    resources = {
      cpu_time_limit_ms = 5000
      memory_limit_bytes = 67108864
    }
  }
]
"#,
        )
        .unwrap();

        let compiled = spec.compile().unwrap();
        let service = compiled
            .manifest
            .services
            .iter()
            .find(|service| service.name == "api")
            .unwrap();
        assert_eq!(service.resources.memory_limit_bytes, Some(64 * 1024 * 1024));
        assert!(
            !compiled
                .manifest
                .render()
                .unwrap()
                .contains("cpu_time_limit_ms")
        );
    }

    #[test]
    fn render_round_trips_internal_only_service_without_exposure() {
        let manifest = ProjectManifest {
            project: ProjectConfig {
                name: "demo".to_owned(),
                domain: None,
            },
            services: vec![ServiceManifest {
                name: "api".to_owned(),
                kind: ServiceKind::Http,
                path: PathBuf::from("services/api"),
                prefix: "/_ignis_internal/api".to_owned(),
                agent_runtime: AgentRuntime::Codex,
                http: Some(HttpServiceConfig {
                    component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                    base_path: "/".to_owned(),
                }),
                frontend: None,
                agent: None,
                ignis_login: None,
                env: BTreeMap::new(),
                secrets: BTreeMap::new(),
                sqlite: SqliteConfig::default(),
                resources: ResourceConfig::default(),
            }],
            jobs: Vec::new(),
            schedules: Vec::new(),
        };

        let rendered = ProjectSpec::from_project_manifest(&manifest).unwrap();
        assert!(rendered.exposes.is_empty());
        assert!(rendered.listeners.is_empty());
        assert_eq!(rendered.services.len(), 1);
    }

    #[test]
    fn validates_jobs_and_schedules_against_services() {
        let spec = ProjectSpec {
            project: ProjectConfig {
                name: "demo".to_owned(),
                domain: None,
            },
            listeners: vec![ListenerSpec {
                name: "public".to_owned(),
                protocol: ListenerProtocol::Http,
            }],
            exposes: vec![ExposeSpec {
                name: "api".to_owned(),
                listener: "public".to_owned(),
                service: "api".to_owned(),
                binding: Some("http".to_owned()),
                path: "/api".to_owned(),
            }],
            services: vec![ServiceSpec {
                name: "api".to_owned(),
                kind: ServiceKind::Http,
                path: PathBuf::from("services/api"),
                agent_runtime: AgentRuntime::Codex,
                bindings: vec![BindingSpec {
                    name: "http".to_owned(),
                    kind: BindingKind::Http,
                }],
                http: Some(HttpServiceConfig {
                    component: PathBuf::from("target/wasm32-wasip2/release/api.wasm"),
                    base_path: "/".to_owned(),
                }),
                frontend: None,
                agent: None,
                ignis_login: None,
                env: BTreeMap::new(),
                secrets: BTreeMap::new(),
                sqlite: SqliteConfig::default(),
                resources: ResourceConfig::default(),
            }],
            jobs: vec![JobSpec {
                name: "ocr_receipt".to_owned(),
                queue: "default".to_owned(),
                target: crate::JobTargetSpec {
                    service: "api".to_owned(),
                    binding: Some("http".to_owned()),
                    path: "/jobs/ocr".to_owned(),
                    method: "POST".to_owned(),
                },
                timeout_ms: Some(60_000),
                retry: crate::JobRetrySpec {
                    max_attempts: 3,
                    backoff: crate::JobRetryBackoff::Exponential,
                    initial_delay_ms: Some(1_000),
                    max_delay_ms: Some(10_000),
                },
                concurrency: crate::JobConcurrencySpec {
                    max_running: Some(2),
                },
                retention: crate::JobRetentionSpec {
                    keep_success_days: Some(7),
                    keep_failed_days: Some(30),
                },
            }],
            schedules: vec![crate::ScheduleSpec {
                name: "daily_ocr_digest".to_owned(),
                job: "ocr_receipt".to_owned(),
                cron: "0 8 * * *".to_owned(),
                timezone: "Asia/Shanghai".to_owned(),
                enabled: true,
                overlap_policy: crate::ScheduleOverlapPolicy::Forbid,
                misfire_policy: crate::ScheduleMisfirePolicy::RunOnce,
                input: serde_json::json!({
                    "batch": "receipts"
                }),
            }],
        };

        let compiled = spec.compile().unwrap();
        assert_eq!(compiled.manifest.jobs.len(), 1);
        assert_eq!(compiled.manifest.schedules.len(), 1);
        assert_eq!(compiled.manifest.jobs[0].name, "ocr_receipt");
        assert_eq!(compiled.manifest.schedules[0].job, "ocr_receipt");
    }
}
