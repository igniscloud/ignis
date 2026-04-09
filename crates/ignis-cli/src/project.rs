use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ignis_manifest::{PROJECT_MANIFEST_FILE, ProjectConfig, ProjectManifest, ServiceManifest};
use serde::Serialize;

use crate::api::ApiClient;
use crate::cli::{ProjectCommands, ProjectTokenCommands};
use crate::config;
use crate::context::ProjectContext;
use crate::output::{self, Drift};
use crate::service;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ProjectSyncServiceResult {
    service: String,
    status: &'static str,
    message: String,
}

#[derive(Debug, serde::Deserialize)]
struct ProjectServicesEnvelope {
    data: Vec<RemoteServiceEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct RemoteServiceEntry {
    service: String,
    manifest: ServiceManifest,
}

pub async fn handle(command: ProjectCommands, token: Option<String>) -> Result<()> {
    match command {
        ProjectCommands::Create { name, dir, force } => create_project(name, dir, force, token).await,
        ProjectCommands::Sync => sync_project(token).await,
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
        project: ProjectConfig { name },
        services: Vec::new(),
    };
    let manifest_path = target_dir.join(PROJECT_MANIFEST_FILE);
    fs::write(&manifest_path, manifest.render()?)
        .with_context(|| format!("writing {}", manifest_path.display()))?;

    output::success(serde_json::json!({
        "remote": response,
        "local": {
            "project_dir": target_dir,
            "project_manifest_path": manifest_path,
        }
    }))
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

async fn sync_project(token: Option<String>) -> Result<()> {
    let context = ProjectContext::load()?;
    for service in &context.manifest().services {
        service::ensure_service_check_passes(service)?;
    }

    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let project_name = context.project_name().to_owned();
    let mut project_created = false;
    if client.project_status_optional(&project_name).await?.is_none() {
        client.create_project(&project_name).await?;
        project_created = true;
    }

    let remote_services: ProjectServicesEnvelope =
        serde_json::from_value(client.services(&project_name).await?)
            .context("parsing project services response")?;
    let remote_manifests = remote_services
        .data
        .into_iter()
        .map(|entry| (entry.service.clone(), entry.manifest))
        .collect::<BTreeMap<_, _>>();

    let mut service_results = Vec::new();
    let mut local_service_names = BTreeSet::new();
    let mut drift = Vec::new();

    for service in &context.manifest().services {
        local_service_names.insert(service.name.clone());
        match remote_manifests.get(&service.name) {
            None => {
                client.create_service(&project_name, service).await?;
                service_results.push(ProjectSyncServiceResult {
                    service: service.name.clone(),
                    status: "created",
                    message: format!("created remote service `{}`", service.name),
                });
            }
            Some(remote_manifest) if remote_manifest == service => {
                service_results.push(ProjectSyncServiceResult {
                    service: service.name.clone(),
                    status: "unchanged",
                    message: format!(
                        "remote service `{}` already matches local manifest",
                        service.name
                    ),
                });
            }
            Some(_) => {
                let message = format!(
                    "remote service `{}` already exists but its manifest differs; current sync only creates missing services",
                    service.name
                );
                service_results.push(ProjectSyncServiceResult {
                    service: service.name.clone(),
                    status: "drift",
                    message: message.clone(),
                });
                drift.push(Drift::for_service(
                    "service_manifest_drift",
                    service.name.clone(),
                    message,
                ));
            }
        }
    }

    let remote_only_services = remote_manifests
        .keys()
        .filter(|name| !local_service_names.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    drift.extend(remote_only_services.iter().map(|service| {
        Drift::for_service(
            "remote_only_service",
            service.clone(),
            format!(
                "remote service `{service}` is not declared locally and was left unchanged"
            ),
        )
    }));

    output::success_with(
        serde_json::json!({
            "project": project_name,
            "project_created": project_created,
            "service_results": service_results,
            "remote_only_services": remote_only_services,
        }),
        Vec::new(),
        drift,
    )
}
