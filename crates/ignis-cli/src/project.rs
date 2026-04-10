use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use ignis_manifest::{
    BindingKind, CompiledServicePlan, PROJECT_MANIFEST_FILE, ProjectConfig, ProjectManifest,
    ServiceKind, ServiceManifest,
};
use serde::Serialize;
use serde_json::Value;

use crate::api::ApiClient;
use crate::cli::{ProjectCommands, ProjectSyncMode, ProjectTokenCommands};
use crate::config;
use crate::context::ProjectContext;
use crate::output::{self, CliError, Drift, Warning};
use crate::project_state::project_state_from_response;
use crate::service;

#[derive(Debug, serde::Deserialize)]
struct ProjectServicesEnvelope {
    data: Vec<RemoteServiceEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct RemoteServiceEntry {
    service: String,
    manifest: ServiceManifest,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct SyncPlan {
    mode: &'static str,
    project: String,
    actions: Vec<SyncAction>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct SyncAction {
    kind: &'static str,
    status: &'static str,
    apply_supported: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    service: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    diffs: Vec<ManifestFieldDiff>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct ManifestFieldDiff {
    path: String,
    local: Value,
    remote: Value,
}

pub async fn handle(command: ProjectCommands, token: Option<String>) -> Result<()> {
    match command {
        ProjectCommands::Create { name, dir, force } => {
            create_project(name, dir, force, token).await
        }
        ProjectCommands::Sync { mode } => sync_project(mode, token).await,
        ProjectCommands::List => list_projects(token).await,
        ProjectCommands::Status { project } => project_status(&project, token).await,
        ProjectCommands::Delete { project } => delete_project(&project, token).await,
        ProjectCommands::Token { command } => project_token_command(command, token).await,
    }
}

async fn project_token_command(command: ProjectTokenCommands, token: Option<String>) -> Result<()> {
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = match command {
        ProjectTokenCommands::Create {
            project,
            issued_for,
        } => {
            client
                .create_project_token(&project, issued_for.as_deref())
                .await?
        }
        ProjectTokenCommands::Revoke { project, token_id } => {
            client.revoke_project_token(&project, &token_id).await?
        }
    };
    output::success(response)
}

async fn create_project(
    name: String,
    dir: Option<PathBuf>,
    force: bool,
    token: Option<String>,
) -> Result<()> {
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.create_project(&name).await?;

    let target_dir = dir.unwrap_or_else(|| PathBuf::from(&name));
    ensure_project_dir_ready(&target_dir, force)?;
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("creating {}", target_dir.display()))?;
    let manifest = ProjectManifest {
        project: ProjectConfig { name: name.clone() },
        services: Vec::new(),
    };
    let manifest_path = target_dir.join(PROJECT_MANIFEST_FILE);
    fs::write(&manifest_path, manifest.render()?)
        .with_context(|| format!("writing {}", manifest_path.display()))?;
    let project_state = project_state_from_response(&response, &name);
    let project_state_path = project_state.save(&target_dir)?;
    let mut warnings = Vec::new();
    if project_state.project_id().is_none() {
        warnings.push(Warning::new(
            "project_create_response_missing_project_id",
            format!(
                "control-plane create-project response for `{}` did not include a project_id; only the local project name was saved, so future remote operations may require re-linking once a project_id-aware API is available",
                project_state.project_name
            ),
        ));
    }

    output::success_with(
        serde_json::json!({
            "remote": response,
            "local": {
                "project_dir": target_dir,
                "project_manifest_path": manifest_path,
                "project_state_path": project_state_path,
                "project_id": project_state.project_id(),
            }
        }),
        warnings,
        Vec::new(),
    )
}

fn ensure_project_dir_ready(path: &Path, force: bool) -> Result<()> {
    if path.exists() {
        let mut entries = path
            .read_dir()
            .with_context(|| format!("reading {}", path.display()))?;
        if entries.next().is_some() && !force {
            bail!(
                "directory {} is not empty; pass --force to overwrite the project manifest",
                path.display()
            );
        }
    }
    Ok(())
}

async fn list_projects(token: Option<String>) -> Result<()> {
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.projects().await?;
    output::success(response)
}

async fn project_status(project: &str, token: Option<String>) -> Result<()> {
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.project_status(project).await?;
    output::success(response)
}

async fn delete_project(project: &str, token: Option<String>) -> Result<()> {
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.delete_project(project).await?;
    output::success(response)
}

async fn sync_project(mode: ProjectSyncMode, token: Option<String>) -> Result<()> {
    let context = ProjectContext::load()?;
    for service in &context.manifest().services {
        service::ensure_service_check_passes(service)?;
    }

    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let project_name = context.project_name().to_owned();
    let project_id = context.project_id().map(str::to_owned);
    let project_missing = match project_id.as_deref() {
        Some(project_id) => client.project_status_optional(project_id).await?.is_none(),
        None => true,
    };
    let remote_manifests =
        fetch_remote_manifests(&client, project_id.as_deref(), project_missing).await?;
    let plan = build_sync_plan(
        &context,
        &project_name,
        project_id.as_deref(),
        project_missing,
        &remote_manifests,
    )?;

    match mode {
        ProjectSyncMode::Plan => output_plan(plan),
        ProjectSyncMode::Apply => apply_sync_plan(&client, &context, plan).await,
    }
}

async fn fetch_remote_manifests(
    client: &ApiClient,
    project_id: Option<&str>,
    project_missing: bool,
) -> Result<BTreeMap<String, ServiceManifest>> {
    if project_missing {
        return Ok(BTreeMap::new());
    }

    let project_id =
        project_id.ok_or_else(|| anyhow!("project_id is required to fetch remote manifests"))?;

    let remote_services: ProjectServicesEnvelope =
        serde_json::from_value(client.services(project_id).await?)
            .context("parsing project services response")?;
    Ok(remote_services
        .data
        .into_iter()
        .map(|entry| (entry.service.clone(), entry.manifest))
        .collect::<BTreeMap<_, _>>())
}

fn build_sync_plan(
    context: &ProjectContext,
    project_name: &str,
    project_id: Option<&str>,
    project_missing: bool,
    remote_manifests: &BTreeMap<String, ServiceManifest>,
) -> Result<SyncPlan> {
    let mut actions = Vec::new();
    let compiled_services = context
        .compiled_plan()
        .services
        .iter()
        .map(|service| (service.name.as_str(), service))
        .collect::<BTreeMap<_, _>>();

    if project_missing {
        let message = match project_id {
            Some(project_id) => format!(
                "remote project binding `{project_id}` for local project `{project_name}` is missing and will be created again"
            ),
            None => format!(
                "local project `{project_name}` is not linked to a remote project yet; apply will create one and save its project_id to `.ignis/project.json`"
            ),
        };
        actions.push(SyncAction {
            kind: "create_project",
            status: "planned",
            apply_supported: true,
            message,
            service: None,
            diffs: Vec::new(),
        });
    }

    let mut local_service_names = BTreeSet::new();
    for service in &context.manifest().services {
        local_service_names.insert(service.name.clone());
        let compiled_service = compiled_services
            .get(service.name.as_str())
            .ok_or_else(|| {
                anyhow!(
                    "compiled plan is missing service `{}` declared in {}",
                    service.name,
                    context.manifest_path().display()
                )
            })?;
        if let Some(message) = unsupported_remote_binding_message(compiled_service) {
            actions.push(SyncAction {
                kind: "unsupported_compiled_plan",
                status: "blocked",
                apply_supported: false,
                message,
                service: Some(service.name.clone()),
                diffs: Vec::new(),
            });
            continue;
        }
        match remote_manifests.get(&service.name) {
            None => {
                actions.push(SyncAction {
                    kind: "create_service",
                    status: "planned",
                    apply_supported: true,
                    message: format!(
                        "remote service `{}` is missing and will be created",
                        service.name
                    ),
                    service: Some(service.name.clone()),
                    diffs: Vec::new(),
                });
            }
            Some(remote_manifest) if remote_manifest == service => {
                actions.push(SyncAction {
                    kind: "noop",
                    status: "noop",
                    apply_supported: false,
                    message: format!(
                        "remote service `{}` already matches the local manifest",
                        service.name
                    ),
                    service: Some(service.name.clone()),
                    diffs: Vec::new(),
                });
            }
            Some(remote_manifest) => {
                actions.push(SyncAction {
                    kind: "repair_service_manifest",
                    status: "blocked",
                    apply_supported: false,
                    message: format!(
                        "remote service `{}` differs from the local manifest; inspect the field-level diff before a future repair path is implemented",
                        service.name
                    ),
                    service: Some(service.name.clone()),
                    diffs: diff_service_manifests(service, remote_manifest)?,
                });
            }
        }
    }

    for service in remote_manifests
        .keys()
        .filter(|name| !local_service_names.contains(*name))
    {
        actions.push(SyncAction {
            kind: "remote_only_service",
            status: "noop",
            apply_supported: false,
            message: format!(
                "remote service `{service}` is not declared locally and will be left unchanged"
            ),
            service: Some(service.clone()),
            diffs: Vec::new(),
        });
    }

    Ok(SyncPlan {
        mode: "plan",
        project: project_name.to_owned(),
        actions,
    })
}

fn output_plan(plan: SyncPlan) -> Result<()> {
    let (warnings, drift) = plan_advisories(&plan);
    output::success_with(
        serde_json::json!({
            "mode": plan.mode,
            "project": plan.project,
            "actions": plan.actions,
        }),
        warnings,
        drift,
    )
}

async fn apply_sync_plan(
    client: &ApiClient,
    context: &ProjectContext,
    plan: SyncPlan,
) -> Result<()> {
    let mut applied_actions = Vec::new();
    let mut project_created = false;
    let mut remote_project_id = context.project_id().map(str::to_owned);
    let mut project_state_path = context.project_dir().join(".ignis/project.json");

    for action in &plan.actions {
        match action.kind {
            "create_project" => {
                let response = client.create_project(&plan.project).await?;
                let project_state = project_state_from_response(&response, &plan.project);
                project_state_path = project_state.save(context.project_dir())?;
                remote_project_id = project_state.project_id().map(str::to_owned);
                project_created = true;
                applied_actions.push(SyncAction {
                    status: "applied",
                    ..action.clone()
                });
            }
            "create_service" => {
                let service_name = action
                    .service
                    .as_deref()
                    .ok_or_else(|| anyhow!("create_service action is missing service name"))?;
                let service = context.find_service(service_name).ok_or_else(|| {
                    anyhow!("local service `{service_name}` not found while applying sync plan")
                })?;
                let project_id = remote_project_id.as_deref().ok_or_else(|| {
                    CliError::new(format!(
                        "project `{}` is still not linked to a remote project_id after creation",
                        plan.project
                    ))
                    .code("project_id_missing")
                    .with_details([
                        format!(
                            "expected `.ignis/project.json` at {} to contain a `project_id` after `create_project`",
                            project_state_path.display()
                        ),
                        "the control-plane create-project response must include `data.project_id` for follow-up service operations".to_owned(),
                    ])
                })?;
                client.create_service(project_id, service).await?;
                applied_actions.push(SyncAction {
                    status: "applied",
                    ..action.clone()
                });
            }
            "repair_service_manifest"
            | "noop"
            | "remote_only_service"
            | "unsupported_compiled_plan" => {
                applied_actions.push(action.clone());
            }
            other => bail!("unsupported sync action `{other}`"),
        }
    }

    let applied_plan = SyncPlan {
        mode: "apply",
        project: plan.project,
        actions: applied_actions,
    };
    let (warnings, drift) = plan_advisories(&applied_plan);
    output::success_with(
        serde_json::json!({
            "mode": applied_plan.mode,
            "project": applied_plan.project,
            "project_created": project_created,
            "actions": applied_plan.actions,
        }),
        warnings,
        drift,
    )
}

fn plan_advisories(plan: &SyncPlan) -> (Vec<Warning>, Vec<Drift>) {
    let mut warnings = Vec::new();
    let mut drift = Vec::new();

    for action in &plan.actions {
        match action.kind {
            "unsupported_compiled_plan" => {
                if let Some(service) = &action.service {
                    warnings.push(Warning::new(
                        "compiled_plan_remote_sync_unsupported",
                        format!(
                            "service `{service}` uses ISL bindings that the current control-plane sync flow cannot represent; publish/deploy this service through the legacy per-service path only after the deployment API is upgraded"
                        ),
                    ));
                    drift.push(Drift::for_service(
                        "compiled_plan_remote_sync_unsupported",
                        service.clone(),
                        action.message.clone(),
                    ));
                }
            }
            "repair_service_manifest" => {
                if let Some(service) = &action.service {
                    drift.push(Drift::for_service(
                        "service_manifest_drift",
                        service.clone(),
                        action.message.clone(),
                    ));
                }
            }
            "remote_only_service" => {
                if let Some(service) = &action.service {
                    warnings.push(Warning::new(
                        "remote_only_service",
                        format!(
                            "remote service `{service}` is not declared locally and will be left unchanged"
                        ),
                    ));
                }
            }
            "create_project" if action.message.contains(".ignis/project.json") => {
                warnings.push(Warning::new("project_not_linked", action.message.clone()));
            }
            _ => {}
        }
    }

    (warnings, drift)
}

fn unsupported_remote_binding_message(service: &CompiledServicePlan) -> Option<String> {
    let expected = match service.kind {
        ServiceKind::Http => ("http", BindingKind::Http),
        ServiceKind::Frontend => ("frontend", BindingKind::Frontend),
    };
    if service.bindings.len() != 1 {
        let bindings = service
            .bindings
            .iter()
            .map(|binding| format!("{}:{}", binding.name, binding.protocol.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        return Some(format!(
            "service `{}` compiles to multiple bindings [{bindings}], but remote project sync still only supports one default binding per service",
            service.name
        ));
    }
    let binding = &service.bindings[0];
    if binding.name != expected.0 || binding.protocol != expected.1 {
        return Some(format!(
            "service `{}` compiles to binding `{}` kind `{}`, but remote project sync only supports the default `{}` `{}` binding for service kind `{}`",
            service.name,
            binding.name,
            binding.protocol.as_str(),
            expected.0,
            expected.1.as_str(),
            service.kind.as_str(),
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ignis_manifest::CompiledBindingPlan;

    fn compiled_service(
        kind: ServiceKind,
        bindings: Vec<(&str, BindingKind)>,
    ) -> CompiledServicePlan {
        CompiledServicePlan {
            name: "api".to_owned(),
            kind,
            path: PathBuf::from("services/api"),
            service_identity: "svc://demo/api".to_owned(),
            bindings: bindings
                .into_iter()
                .map(|(name, protocol)| CompiledBindingPlan {
                    name: name.to_owned(),
                    binding_identity: format!("svc://demo/api#{name}"),
                    protocol,
                    public_exposures: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn accepts_default_http_binding_for_remote_sync() {
        let service = compiled_service(ServiceKind::Http, vec![("http", BindingKind::Http)]);
        assert_eq!(unsupported_remote_binding_message(&service), None);
    }

    #[test]
    fn blocks_multi_binding_service_for_remote_sync() {
        let service = compiled_service(
            ServiceKind::Http,
            vec![("http", BindingKind::Http), ("internal", BindingKind::Http)],
        );
        let message =
            unsupported_remote_binding_message(&service).expect("message should be present");
        assert!(message.contains("multiple bindings"));
        assert!(message.contains("http:http"));
        assert!(message.contains("internal:http"));
    }

    #[test]
    fn blocks_non_default_binding_for_remote_sync() {
        let service = compiled_service(ServiceKind::Http, vec![("internal", BindingKind::Http)]);
        let message =
            unsupported_remote_binding_message(&service).expect("message should be present");
        assert!(message.contains("binding `internal` kind `http`"));
    }
}

fn diff_service_manifests(
    local: &ServiceManifest,
    remote: &ServiceManifest,
) -> Result<Vec<ManifestFieldDiff>> {
    let local_value = serde_json::to_value(local).context("serializing local service manifest")?;
    let remote_value =
        serde_json::to_value(remote).context("serializing remote service manifest")?;
    let mut diffs = Vec::new();
    collect_value_diffs("", &local_value, &remote_value, &mut diffs);
    Ok(diffs)
}

fn collect_value_diffs(
    path: &str,
    local: &Value,
    remote: &Value,
    diffs: &mut Vec<ManifestFieldDiff>,
) {
    match (local, remote) {
        (Value::Object(local_map), Value::Object(remote_map)) => {
            let keys = local_map
                .keys()
                .chain(remote_map.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            for key in keys {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match (local_map.get(&key), remote_map.get(&key)) {
                    (Some(local_value), Some(remote_value)) => {
                        collect_value_diffs(&child_path, local_value, remote_value, diffs);
                    }
                    (Some(local_value), None) => diffs.push(ManifestFieldDiff {
                        path: child_path,
                        local: local_value.clone(),
                        remote: Value::Null,
                    }),
                    (None, Some(remote_value)) => diffs.push(ManifestFieldDiff {
                        path: child_path,
                        local: Value::Null,
                        remote: remote_value.clone(),
                    }),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(local_items), Value::Array(remote_items)) => {
            let max_len = std::cmp::max(local_items.len(), remote_items.len());
            for index in 0..max_len {
                let child_path = format!("{path}[{index}]");
                match (local_items.get(index), remote_items.get(index)) {
                    (Some(local_value), Some(remote_value)) => {
                        collect_value_diffs(&child_path, local_value, remote_value, diffs);
                    }
                    (Some(local_value), None) => diffs.push(ManifestFieldDiff {
                        path: child_path,
                        local: local_value.clone(),
                        remote: Value::Null,
                    }),
                    (None, Some(remote_value)) => diffs.push(ManifestFieldDiff {
                        path: child_path,
                        local: Value::Null,
                        remote: remote_value.clone(),
                    }),
                    (None, None) => {}
                }
            }
        }
        _ => {
            if local != remote {
                diffs.push(ManifestFieldDiff {
                    path: path.to_owned(),
                    local: local.clone(),
                    remote: remote.clone(),
                });
            }
        }
    }
}

pub(crate) fn linked_project_id(context: &ProjectContext) -> Result<&str> {
    context.project_id().ok_or_else(|| {
        CliError::new(format!(
            "project `{}` is not linked to a remote project_id",
            context.project_name()
        ))
        .code("project_not_linked")
        .with_details([
            format!(
                "run `ignis project sync --mode apply` in {} to create a remote project and save `.ignis/project.json`",
                context.project_dir().display()
            ),
            "remote service operations no longer fall back to `project.name`, because that could target another project with the same name".to_owned(),
        ])
        .into()
    })
}
