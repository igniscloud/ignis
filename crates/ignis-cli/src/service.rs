use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ignis_manifest::{
    FrontendServiceConfig, HttpServiceConfig, IGNIS_LOGIN_IGNISCLOUD_ID_BASE_URL_ENV,
    ResourceConfig, ServiceKind, ServiceManifest, SqliteConfig,
};
use serde::Serialize;

use crate::api::ApiClient;
use crate::build;
use crate::cli::{
    CliServiceKind, ServiceCommands, ServiceEnvCommands, ServiceSecretCommands,
    ServiceSqliteCommands,
};
use crate::config;
use crate::context::ProjectContext;
use crate::output::{self, CliError, Warning};
use crate::project::linked_project_id;
use crate::template;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ServiceCheckFinding {
    level: &'static str,
    code: &'static str,
    message: String,
}

pub async fn handle(command: ServiceCommands, token: Option<String>) -> Result<()> {
    let context = ProjectContext::load()?;
    match command {
        ServiceCommands::New {
            service,
            kind,
            path,
        } => new_service(&context, &service, kind, &path, token).await,
        ServiceCommands::List => list_services(&context),
        ServiceCommands::Status { service } => service_status(&context, &service, token).await,
        ServiceCommands::Check { service } => check_service(&context, &service),
        ServiceCommands::Delete { service } => delete_service(&context, &service, token).await,
        ServiceCommands::Build { service, release } => {
            build_service_command(&context, &service, release).await
        }
        ServiceCommands::Publish { service } => publish_service(&context, &service, token).await,
        ServiceCommands::Deploy { service, version } => {
            deploy_service(&context, &service, &version, token).await
        }
        ServiceCommands::Deployments { service, limit } => {
            deployments(&context, &service, limit, token).await
        }
        ServiceCommands::Events { service, limit } => {
            events(&context, &service, limit, token).await
        }
        ServiceCommands::Logs { service, limit } => logs(&context, &service, limit, token).await,
        ServiceCommands::Rollback { service, version } => {
            rollback(&context, &service, &version, token).await
        }
        ServiceCommands::DeleteVersion { service, version } => {
            delete_version(&context, &service, &version, token).await
        }
        ServiceCommands::Env { command } => env_command(&context, command, token).await,
        ServiceCommands::Secrets { command } => secret_command(&context, command, token).await,
        ServiceCommands::Sqlite { command } => sqlite_command(&context, command, token).await,
    }
}

async fn new_service(
    context: &ProjectContext,
    service_name: &str,
    kind: CliServiceKind,
    path: &Path,
    token: Option<String>,
) -> Result<()> {
    if context.find_service(service_name).is_some() {
        bail!(
            "service `{service_name}` already exists in {}",
            context.manifest_path().display()
        );
    }

    let service = build_new_service_manifest(service_name, kind, path);
    service.validate()?;
    context.ensure_new_service_path_available(&service)?;
    let project_id = linked_project_id(context)?;

    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.create_service(project_id, &service).await?;

    let mut manifest = context.manifest().clone();
    manifest.services.push(service.clone());
    context.save_manifest(&manifest)?;
    create_local_service_files(context.project_dir(), &service)?;

    output::success(serde_json::json!({
        "remote": response,
        "local": {
            "project": context.project_name(),
            "service": service.name,
            "service_path": path,
        }
    }))
}

fn build_new_service_manifest(
    service_name: &str,
    kind: CliServiceKind,
    path: &Path,
) -> ServiceManifest {
    match kind {
        CliServiceKind::Http => {
            let package_name = service_name.replace('-', "_");
            ServiceManifest {
                name: service_name.to_owned(),
                kind: ServiceKind::Http,
                path: path.to_path_buf(),
                prefix: format!("/{service_name}"),
                http: Some(HttpServiceConfig {
                    component: PathBuf::from(format!(
                        "target/wasm32-wasip2/release/{package_name}.wasm"
                    )),
                    base_path: "/".to_owned(),
                }),
                frontend: None,
                ignis_login: None,
                env: BTreeMap::new(),
                secrets: BTreeMap::new(),
                sqlite: SqliteConfig { enabled: true },
                resources: ResourceConfig {
                    cpu_time_limit_ms: Some(5_000),
                    memory_limit_bytes: Some(128 * 1024 * 1024),
                },
            }
        }
        CliServiceKind::Frontend => ServiceManifest {
            name: service_name.to_owned(),
            kind: ServiceKind::Frontend,
            path: path.to_path_buf(),
            prefix: "/".to_owned(),
            http: None,
            frontend: Some(FrontendServiceConfig {
                build_command: vec![
                    "ignis".to_owned(),
                    "internal".to_owned(),
                    "copy-frontend-static".to_owned(),
                    "--source-dir".to_owned(),
                    "src".to_owned(),
                    "--output-dir".to_owned(),
                    "dist".to_owned(),
                ],
                output_dir: PathBuf::from("dist"),
                spa_fallback: true,
            }),
            ignis_login: None,
            env: BTreeMap::new(),
            secrets: BTreeMap::new(),
            sqlite: SqliteConfig::default(),
            resources: ResourceConfig::default(),
        },
    }
}

fn create_local_service_files(project_dir: &Path, service: &ServiceManifest) -> Result<()> {
    let service_dir = project_dir.join(&service.path);
    fs::create_dir_all(&service_dir)
        .with_context(|| format!("creating {}", service_dir.display()))?;
    match service.kind {
        ServiceKind::Http => {
            let package_name = service.name.replace('-', "_");
            fs::create_dir_all(service_dir.join("src"))
                .with_context(|| format!("creating {}", service_dir.join("src").display()))?;
            fs::create_dir_all(service_dir.join("wit"))
                .with_context(|| format!("creating {}", service_dir.join("wit").display()))?;
            fs::write(
                service_dir.join("Cargo.toml"),
                template::cargo_toml(&service.name),
            )
            .with_context(|| format!("writing {}", service_dir.join("Cargo.toml").display()))?;
            fs::write(service_dir.join("src/lib.rs"), template::lib_rs())
                .with_context(|| format!("writing {}", service_dir.join("src/lib.rs").display()))?;
            fs::write(
                service_dir.join("wit/world.wit"),
                template::world_wit(&package_name),
            )
            .with_context(|| format!("writing {}", service_dir.join("wit/world.wit").display()))?;
            fs::write(service_dir.join(".gitignore"), template::gitignore())
                .with_context(|| format!("writing {}", service_dir.join(".gitignore").display()))?;
        }
        ServiceKind::Frontend => {
            fs::create_dir_all(service_dir.join("src"))
                .with_context(|| format!("creating {}", service_dir.join("src").display()))?;
            fs::write(
                service_dir.join("src/index.html"),
                template::frontend_src_index_html(&service.name),
            )
            .with_context(|| format!("writing {}", service_dir.join("src/index.html").display()))?;
            fs::write(
                service_dir.join(".gitignore"),
                template::frontend_gitignore(),
            )
            .with_context(|| format!("writing {}", service_dir.join(".gitignore").display()))?;
        }
    }
    Ok(())
}

fn list_services(context: &ProjectContext) -> Result<()> {
    let services = context
        .manifest()
        .services
        .iter()
        .map(|service| {
            serde_json::json!({
                "name": service.name,
                "kind": kind_name(service.kind),
                "path": service.path,
                "prefix": service.prefix,
            })
        })
        .collect::<Vec<_>>();

    output::success(serde_json::json!({
        "project": context.project_name(),
        "services": services,
    }))
}

async fn service_status(
    context: &ProjectContext,
    service_name: &str,
    token: Option<String>,
) -> Result<()> {
    let service = context.service(service_name)?;
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let project_id = linked_project_id(context)?;
    let response = client.service_status(project_id, service.name()).await?;
    output::success(response)
}

fn check_service(context: &ProjectContext, service_name: &str) -> Result<()> {
    let service = context.service(service_name)?;
    let findings = collect_service_check_findings(service.manifest());
    let warning_count = findings
        .iter()
        .filter(|finding| finding.level == "warning")
        .count();
    let errors = findings
        .iter()
        .filter(|finding| finding.level == "error")
        .collect::<Vec<_>>();
    let warnings = findings
        .iter()
        .filter(|finding| finding.level == "warning")
        .map(|finding| Warning::new(finding.code, finding.message.clone()))
        .collect::<Vec<_>>();

    if !errors.is_empty() {
        return Err(
            CliError::new(format!("service check failed for `{}`", service.name()))
                .code("service_check_failed")
                .with_warnings(warnings)
                .with_details(
                    errors
                        .into_iter()
                        .map(|finding| format!("[{}] {}", finding.code, finding.message)),
                )
                .into(),
        );
    }

    output::success_with(
        serde_json::json!({
            "project": service.project_name(),
            "service": service.name(),
            "ok": true,
            "error_count": 0,
            "warning_count": warning_count,
            "findings": findings,
        }),
        warnings,
        Vec::new(),
    )
}

pub fn ensure_service_check_passes(service: &ServiceManifest) -> Result<()> {
    let findings = collect_service_check_findings(service);
    let errors = findings
        .iter()
        .filter(|finding| finding.level == "error")
        .collect::<Vec<_>>();
    if errors.is_empty() {
        return Ok(());
    }

    Err(
        CliError::new(format!("service `{}` failed local checks", service.name))
            .code("service_check_failed")
            .with_details(
                errors
                    .into_iter()
                    .map(|finding| format!("[{}] {}", finding.code, finding.message)),
            )
            .into(),
    )
}

pub fn collect_service_check_findings(service: &ServiceManifest) -> Vec<ServiceCheckFinding> {
    let mut findings = Vec::new();

    if service
        .env
        .contains_key(IGNIS_LOGIN_IGNISCLOUD_ID_BASE_URL_ENV)
    {
        findings.push(ServiceCheckFinding {
            level: "error",
            code: "igniscloud_id_base_url_env_not_supported",
            message: format!(
                "service `{}` defines env `{}`; ignis_login should not depend on IGNISCLOUD_ID_BASE_URL as an env var",
                service.name, IGNIS_LOGIN_IGNISCLOUD_ID_BASE_URL_ENV
            ),
        });
    }

    findings
}

async fn delete_service(
    context: &ProjectContext,
    service_name: &str,
    token: Option<String>,
) -> Result<()> {
    let service = context.service(service_name)?;
    let project_id = linked_project_id(context)?;
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.delete_service(project_id, service.name()).await?;

    let mut manifest = context.manifest().clone();
    manifest.services.retain(|item| item.name != service.name());
    context.save_manifest(&manifest)?;

    output::success(response)
}

async fn build_service_command(
    context: &ProjectContext,
    service_name: &str,
    release: bool,
) -> Result<()> {
    let service = context.service(service_name)?;
    let outcome = build::build_service(&service, release).await?;
    output::success(serde_json::json!({
        "project": service.project_name(),
        "service": service.name(),
        "kind": kind_name(service.manifest().kind),
        "mode": outcome.mode,
        "output_path": outcome.output_path,
        "validation": outcome.validation,
    }))
}

async fn publish_service(
    context: &ProjectContext,
    service_name: &str,
    token: Option<String>,
) -> Result<()> {
    let service = context.service(service_name)?;
    ensure_service_check_passes(service.manifest())?;
    let project_id = linked_project_id(context)?;

    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let publish_artifact = build::prepare_publish_artifact(&service).await?;
    let response = match &publish_artifact.kind {
        build::PublishArtifactKind::Http {
            component_path,
            component_signature,
        } => {
            client
                .publish_http_service(
                    project_id,
                    service.name(),
                    service.manifest(),
                    component_path,
                    component_signature.clone(),
                    publish_artifact.metadata.clone(),
                )
                .await?
        }
        build::PublishArtifactKind::Frontend { bundle_path } => {
            client
                .publish_frontend_service(
                    project_id,
                    service.name(),
                    service.manifest(),
                    bundle_path,
                    publish_artifact.metadata.clone(),
                )
                .await?
        }
    };
    let validation = publish_artifact.validation.clone();
    publish_artifact.cleanup();

    output::success(serde_json::json!({
        "validation": validation,
        "remote": response,
    }))
}

async fn deploy_service(
    context: &ProjectContext,
    service_name: &str,
    version: &str,
    token: Option<String>,
) -> Result<()> {
    let service = context.service(service_name)?;
    let project_id = linked_project_id(context)?;
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client
        .deploy_service(project_id, service.name(), version)
        .await?;
    output::success(response)
}

async fn deployments(
    context: &ProjectContext,
    service_name: &str,
    limit: u32,
    token: Option<String>,
) -> Result<()> {
    let service = context.service(service_name)?;
    let project_id = linked_project_id(context)?;
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client
        .deployments(project_id, service.name(), limit)
        .await?;
    output::success(response)
}

async fn events(
    context: &ProjectContext,
    service_name: &str,
    limit: u32,
    token: Option<String>,
) -> Result<()> {
    let service = context.service(service_name)?;
    let project_id = linked_project_id(context)?;
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.events(project_id, service.name(), limit).await?;
    output::success(response)
}

async fn logs(
    context: &ProjectContext,
    service_name: &str,
    limit: u32,
    token: Option<String>,
) -> Result<()> {
    let service = context.service(service_name)?;
    let project_id = linked_project_id(context)?;
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.logs(project_id, service.name(), limit).await?;
    output::success(response)
}

async fn rollback(
    context: &ProjectContext,
    service_name: &str,
    version: &str,
    token: Option<String>,
) -> Result<()> {
    let service = context.service(service_name)?;
    let project_id = linked_project_id(context)?;
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.rollback(project_id, service.name(), version).await?;
    output::success(response)
}

async fn delete_version(
    context: &ProjectContext,
    service_name: &str,
    version: &str,
    token: Option<String>,
) -> Result<()> {
    let service = context.service(service_name)?;
    let project_id = linked_project_id(context)?;
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client
        .delete_version(project_id, service.name(), version)
        .await?;
    output::success(response)
}

async fn env_command(
    context: &ProjectContext,
    command: ServiceEnvCommands,
    token: Option<String>,
) -> Result<()> {
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let project_id = linked_project_id(context)?;
    let response = match command {
        ServiceEnvCommands::List { service } => {
            let service = context.service(&service)?;
            client.env_list(project_id, service.name()).await?
        }
        ServiceEnvCommands::Set {
            service,
            name,
            value,
        } => {
            let service = context.service(&service)?;
            client
                .env_set(project_id, service.name(), &name, &value)
                .await?
        }
        ServiceEnvCommands::Delete { service, name } => {
            let service = context.service(&service)?;
            client.env_delete(project_id, service.name(), &name).await?
        }
    };
    output::success(response)
}

async fn secret_command(
    context: &ProjectContext,
    command: ServiceSecretCommands,
    token: Option<String>,
) -> Result<()> {
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let project_id = linked_project_id(context)?;
    let response = match command {
        ServiceSecretCommands::List { service } => {
            let service = context.service(&service)?;
            client.secrets_list(project_id, service.name()).await?
        }
        ServiceSecretCommands::Set {
            service,
            name,
            value,
        } => {
            let service = context.service(&service)?;
            client
                .secrets_set(project_id, service.name(), &name, &value)
                .await?
        }
        ServiceSecretCommands::Delete { service, name } => {
            let service = context.service(&service)?;
            client
                .secrets_delete(project_id, service.name(), &name)
                .await?
        }
    };
    output::success(response)
}

async fn sqlite_command(
    context: &ProjectContext,
    command: ServiceSqliteCommands,
    token: Option<String>,
) -> Result<()> {
    let client = ApiClient::new(config::CliConfig::resolve(token)?);
    let project_id = linked_project_id(context)?;
    match command {
        ServiceSqliteCommands::Backup { service, out } => {
            let service = context.service(&service)?;
            let bytes = client.sqlite_backup(project_id, service.name()).await?;
            fs::write(&out, &bytes).with_context(|| format!("writing {}", out.display()))?;
            output::success(serde_json::json!({
                "project": service.project_name(),
                "service": service.name(),
                "output_path": out,
                "bytes": bytes.len(),
            }))
        }
        ServiceSqliteCommands::Restore { service, input } => {
            let service = context.service(&service)?;
            let bytes = fs::read(&input).with_context(|| format!("reading {}", input.display()))?;
            let response = client
                .sqlite_restore(project_id, service.name(), &bytes)
                .await?;
            output::success(response)
        }
    }
}

fn kind_name(kind: ServiceKind) -> &'static str {
    match kind {
        ServiceKind::Http => "http",
        ServiceKind::Frontend => "frontend",
    }
}

#[cfg(test)]
mod tests {
    use ignis_manifest::{
        HttpServiceConfig, IgnisLoginConfig, IgnisLoginProvider, ResourceConfig, ServiceKind,
        ServiceManifest, SqliteConfig,
    };
    use std::path::PathBuf;

    use super::collect_service_check_findings;

    fn sample_http_service() -> ServiceManifest {
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
            ignis_login: Some(IgnisLoginConfig {
                display_name: "demo".to_owned(),
                redirect_path: "/auth/callback".to_owned(),
                providers: vec![IgnisLoginProvider::Google],
            }),
            env: Default::default(),
            secrets: Default::default(),
            sqlite: SqliteConfig::default(),
            resources: ResourceConfig::default(),
        }
    }

    #[test]
    fn service_check_flags_igniscloud_id_base_url_env() {
        let mut service = sample_http_service();
        service.env.insert(
            "IGNISCLOUD_ID_BASE_URL".to_owned(),
            "https://id.igniscloud.dev".to_owned(),
        );

        let findings = collect_service_check_findings(&service);

        assert!(
            findings
                .iter()
                .any(|finding| finding.code == "igniscloud_id_base_url_env_not_supported")
        );
    }

    #[test]
    fn service_check_accepts_ignis_login_host_allow_rule() {
        let service = sample_http_service();

        let findings = collect_service_check_findings(&service);

        assert!(findings.is_empty());
    }
}
