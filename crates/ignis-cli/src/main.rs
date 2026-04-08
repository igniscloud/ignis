mod api;
mod config;
mod skill_bundle;
mod template;

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand, ValueEnum};
use ignis_manifest::{
    ComponentSignature, FrontendServiceConfig, HttpServiceConfig, LoadedManifest,
    LoadedProjectManifest, NetworkMode, PROJECT_MANIFEST_FILE, ProjectConfig, ProjectManifest,
    ResourceConfig, ServiceKind, ServiceManifest, SqliteConfig,
    IGNIS_LOGIN_COMMON_SERVER_BASE_URL_ENV, sign_component_with_seed,
};
use serde::Serialize;
use serde_json::Value;
use tokio::process::Command;
use tracing::info;
use tracing_subscriber::EnvFilter;

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Parser)]
#[command(name = "ignis", version, about = "Ignis project and service CLI")]
struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "TOKEN",
        help = "Project token, login token, or API token for igniscloud; also supports IGNIS_TOKEN or IGNISCLOUD_TOKEN"
    )]
    token: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Login,
    Logout,
    Whoami,
    GenSkill {
        #[arg(long, value_enum, default_value_t = SkillFormat::Codex)]
        format: SkillFormat,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },
    Service {
        #[command(subcommand)]
        command: ServiceCommands,
    },
}

#[derive(Debug, Subcommand)]
enum ProjectCommands {
    Create {
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    Sync,
    List,
    Status {
        project: String,
    },
    Delete {
        project: String,
    },
    Token {
        #[command(subcommand)]
        command: ProjectTokenCommands,
    },
}

#[derive(Debug, Subcommand)]
enum ProjectTokenCommands {
    Create {
        project: String,
        #[arg(long)]
        issued_for: Option<String>,
    },
    Revoke {
        project: String,
        token_id: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum SkillFormat {
    Codex,
    Opencode,
    Raw,
}

#[derive(Debug, Subcommand)]
enum ServiceCommands {
    New {
        #[arg(long)]
        service: String,
        #[arg(long)]
        kind: CliServiceKind,
        #[arg(long)]
        path: PathBuf,
    },
    List,
    Status {
        #[arg(long)]
        service: String,
    },
    Check {
        #[arg(long)]
        service: String,
    },
    Delete {
        #[arg(long)]
        service: String,
    },
    Build {
        #[arg(long)]
        service: String,
        #[arg(long, default_value_t = true)]
        release: bool,
    },
    Publish {
        #[arg(long)]
        service: String,
    },
    Deploy {
        #[arg(long)]
        service: String,
        version: String,
    },
    Deployments {
        #[arg(long)]
        service: String,
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    Events {
        #[arg(long)]
        service: String,
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    Logs {
        #[arg(long)]
        service: String,
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    Rollback {
        #[arg(long)]
        service: String,
        version: String,
    },
    DeleteVersion {
        #[arg(long)]
        service: String,
        version: String,
    },
    Env {
        #[command(subcommand)]
        command: ServiceEnvCommands,
    },
    Secrets {
        #[command(subcommand)]
        command: ServiceSecretCommands,
    },
    Sqlite {
        #[command(subcommand)]
        command: ServiceSqliteCommands,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliServiceKind {
    Http,
    Frontend,
}

#[derive(Debug, Subcommand)]
enum ServiceEnvCommands {
    List {
        #[arg(long)]
        service: String,
    },
    Set {
        #[arg(long)]
        service: String,
        name: String,
        value: String,
    },
    Delete {
        #[arg(long)]
        service: String,
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum ServiceSecretCommands {
    List {
        #[arg(long)]
        service: String,
    },
    Set {
        #[arg(long)]
        service: String,
        name: String,
        value: String,
    },
    Delete {
        #[arg(long)]
        service: String,
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum ServiceSqliteCommands {
    Backup {
        #[arg(long)]
        service: String,
        out: PathBuf,
    },
    Restore {
        #[arg(long)]
        service: String,
        input: PathBuf,
    },
}

#[derive(Debug, Clone)]
struct ProjectContext {
    loaded: LoadedProjectManifest,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .without_time()
        .init();

    let Cli { token, command } = Cli::parse();
    match command {
        Commands::Login => login(token).await,
        Commands::Logout => logout(),
        Commands::Whoami => whoami(token).await,
        Commands::GenSkill {
            format,
            path,
            force,
        } => gen_skill(format, path.as_deref(), force),
        Commands::Project { command } => project_command(command, token).await,
        Commands::Service { command } => service_command(command, token).await,
    }
}

async fn project_command(command: ProjectCommands, token: Option<String>) -> Result<()> {
    match command {
        ProjectCommands::Create { name, dir, force } => {
            create_project(name, dir, force, token).await
        }
        ProjectCommands::Sync => sync_project(token).await,
        ProjectCommands::List => list_projects(token).await,
        ProjectCommands::Status { project } => project_status(&project, token).await,
        ProjectCommands::Delete { project } => delete_project(&project, token).await,
        ProjectCommands::Token { command } => project_token_command(command, token).await,
    }
}

async fn service_command(command: ServiceCommands, token: Option<String>) -> Result<()> {
    let context = load_project_context()?;
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
            build_service(&context, &service, release).await
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

async fn project_token_command(command: ProjectTokenCommands, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
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
    print_json(&response)
}

async fn create_project(
    name: String,
    dir: Option<PathBuf>,
    force: bool,
    token: Option<String>,
) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.create_project(&name).await?;

    let target_dir = dir.unwrap_or_else(|| PathBuf::from(&name));
    ensure_project_dir_ready(&target_dir, force)?;
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("creating {}", target_dir.display()))?;
    let manifest = ProjectManifest {
        project: ProjectConfig { name },
        services: Vec::new(),
    };
    fs::write(target_dir.join(PROJECT_MANIFEST_FILE), manifest.render()?).with_context(|| {
        format!(
            "writing {}",
            target_dir.join(PROJECT_MANIFEST_FILE).display()
        )
    })?;
    info!(path = %target_dir.display(), "initialized project root");
    print_json(&response)
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
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.projects().await?;
    print_json(&response)
}

async fn project_status(project: &str, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.project_status(project).await?;
    print_json(&response)
}

async fn delete_project(project: &str, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.delete_project(project).await?;
    print_json(&response)
}

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

async fn sync_project(token: Option<String>) -> Result<()> {
    let context = load_project_context()?;
    for service in &context.loaded.manifest.services {
        ensure_service_check_passes(service)?;
    }

    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let project_name = context.loaded.project_name().to_owned();
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
    let mut local_service_names = std::collections::BTreeSet::new();
    for service in &context.loaded.manifest.services {
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
                    message: format!("remote service `{}` already matches local manifest", service.name),
                });
            }
            Some(_) => {
                service_results.push(ProjectSyncServiceResult {
                    service: service.name.clone(),
                    status: "drift",
                    message: format!(
                        "remote service `{}` already exists but its manifest differs; current sync only creates missing services",
                        service.name
                    ),
                });
            }
        }
    }

    let remote_only_services = remote_manifests
        .keys()
        .filter(|name| !local_service_names.contains(*name))
        .cloned()
        .collect::<Vec<_>>();

    print_json(&serde_json::json!({
        "data": {
            "project": project_name,
            "project_created": project_created,
            "service_results": service_results,
            "remote_only_services": remote_only_services,
        }
    }))
}

async fn new_service(
    context: &ProjectContext,
    service_name: &str,
    kind: CliServiceKind,
    path: &Path,
    token: Option<String>,
) -> Result<()> {
    if context.loaded.find_service(service_name).is_some() {
        bail!(
            "service `{service_name}` already exists in {}",
            context.loaded.manifest_path.display()
        );
    }

    let service = build_new_service_manifest(service_name, kind, path);
    service.validate()?;
    validate_new_service_path(context, &service)?;

    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client
        .create_service(context.loaded.project_name(), &service)
        .await?;

    let mut manifest = context.loaded.manifest.clone();
    manifest.services.push(service.clone());
    save_project_manifest(&context.loaded.manifest_path, &manifest)?;
    create_local_service_files(&context.loaded.project_dir, &service)?;

    print_json(&response)
}

fn gen_skill(format: SkillFormat, path: Option<&Path>, force: bool) -> Result<()> {
    let root = gen_skill_output_root(format, path);
    if root.exists() {
        if !force {
            bail!(
                "{} already exists; pass --force to overwrite",
                root.display()
            );
        }
        if root.is_dir() {
            fs::remove_dir_all(&root).with_context(|| format!("removing {}", root.display()))?;
        } else {
            fs::remove_file(&root).with_context(|| format!("removing {}", root.display()))?;
        }
    }

    match format {
        SkillFormat::Codex | SkillFormat::Opencode => {
            write_bundled_skill_dir(&root, "SKILL.md", skill_bundle::ignis_user_files())?;
            let entrypoint = root.join("SKILL.md");
            print_json(&serde_json::json!({
                "data": {
                    "format": skill_format_name(format),
                    "name": "ignis-user",
                    "path": root,
                    "entrypoint": entrypoint,
                }
            }))
        }
        SkillFormat::Raw => {
            fs::create_dir_all(root.join("references"))
                .with_context(|| format!("creating {}", root.join("references").display()))?;
            for file in skill_bundle::ignis_user_files() {
                if file.path == "SKILL.md" {
                    continue;
                }
                let destination = root.join(file.path);
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("creating {}", parent.display()))?;
                }
                fs::write(&destination, file.contents)
                    .with_context(|| format!("writing {}", destination.display()))?;
            }
            let entrypoint = root.join("skill.md");
            fs::write(&entrypoint, skill_bundle::raw_ignis_user_markdown())
                .with_context(|| format!("writing {}", entrypoint.display()))?;
            print_json(&serde_json::json!({
                "data": {
                    "format": skill_format_name(format),
                    "name": "ignis-user",
                    "path": root,
                    "entrypoint": entrypoint,
                }
            }))
        }
    }
}

fn gen_skill_output_root(format: SkillFormat, path: Option<&Path>) -> PathBuf {
    if let Some(path) = path {
        return path.to_path_buf();
    }
    match format {
        SkillFormat::Codex => PathBuf::from(".codex").join("skills").join("ignis-user"),
        SkillFormat::Opencode => PathBuf::from(".opencode").join("skills").join("ignis-user"),
        SkillFormat::Raw => PathBuf::from("ignis-user"),
    }
}

fn skill_format_name(format: SkillFormat) -> &'static str {
    match format {
        SkillFormat::Codex => "codex",
        SkillFormat::Opencode => "opencode",
        SkillFormat::Raw => "raw",
    }
}

fn write_bundled_skill_dir(
    root: &Path,
    entrypoint_name: &str,
    files: &[skill_bundle::BundledFile],
) -> Result<()> {
    fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
    for file in files {
        let relative_path = if file.path == "SKILL.md" {
            PathBuf::from(entrypoint_name)
        } else {
            PathBuf::from(file.path)
        };
        let destination = root.join(relative_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::write(&destination, file.contents)
            .with_context(|| format!("writing {}", destination.display()))?;
    }
    Ok(())
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
                network: Default::default(),
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
                    "bash".to_owned(),
                    "-lc".to_owned(),
                    "rm -rf dist && mkdir -p dist && cp -R src/. dist/".to_owned(),
                ],
                output_dir: PathBuf::from("dist"),
                spa_fallback: true,
            }),
            ignis_login: None,
            env: BTreeMap::new(),
            secrets: BTreeMap::new(),
            sqlite: SqliteConfig::default(),
            resources: ResourceConfig::default(),
            network: Default::default(),
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

fn validate_new_service_path(context: &ProjectContext, service: &ServiceManifest) -> Result<()> {
    let new_path = normalized_relative_path(&service.path);
    for existing in &context.loaded.manifest.services {
        if normalized_relative_path(&existing.path) == new_path {
            bail!(
                "service path `{}` is already used by service `{}`",
                service.path.display(),
                existing.name
            );
        }
    }

    let service_dir = context.loaded.project_dir.join(&service.path);
    if service_dir.exists() {
        let metadata = fs::metadata(&service_dir)
            .with_context(|| format!("reading {}", service_dir.display()))?;
        if !metadata.is_dir() {
            bail!(
                "service path `{}` already exists and is not a directory",
                service_dir.display()
            );
        }
        let mut entries = service_dir
            .read_dir()
            .with_context(|| format!("reading {}", service_dir.display()))?;
        if entries.next().is_some() {
            bail!(
                "service path `{}` already exists and is not empty",
                service_dir.display()
            );
        }
    }

    Ok(())
}

fn normalized_relative_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn list_services(context: &ProjectContext) -> Result<()> {
    let names = context
        .loaded
        .manifest
        .services
        .iter()
        .map(|service| {
            serde_json::json!({
                "name": service.name,
                "kind": match service.kind {
                    ServiceKind::Http => "http",
                    ServiceKind::Frontend => "frontend",
                },
                "path": service.path,
                "prefix": service.prefix,
            })
        })
        .collect::<Vec<_>>();
    print_json(&serde_json::json!({
        "data": {
            "project": context.loaded.project_name(),
            "services": names,
        }
    }))
}

async fn service_status(
    context: &ProjectContext,
    service: &str,
    token: Option<String>,
) -> Result<()> {
    ensure_service_exists(&context.loaded, service)?;
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client
        .service_status(context.loaded.project_name(), service)
        .await?;
    print_json(&response)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ServiceCheckFinding {
    level: &'static str,
    code: &'static str,
    message: String,
}

fn check_service(context: &ProjectContext, service_name: &str) -> Result<()> {
    let service = required_service(&context.loaded, service_name)?;
    let findings = collect_service_check_findings(service);
    let error_count = findings.iter().filter(|finding| finding.level == "error").count();
    let warning_count = findings
        .iter()
        .filter(|finding| finding.level == "warning")
        .count();
    let ok = error_count == 0;

    print_json(&serde_json::json!({
        "data": {
            "project": context.loaded.project_name(),
            "service": service.name,
            "ok": ok,
            "error_count": error_count,
            "warning_count": warning_count,
            "findings": findings,
        }
    }))?;

    if ok {
        Ok(())
    } else {
        bail!("service check failed for `{}`", service.name);
    }
}

fn collect_service_check_findings(service: &ServiceManifest) -> Vec<ServiceCheckFinding> {
    let mut findings = Vec::new();

    if service
        .env
        .contains_key(IGNIS_LOGIN_COMMON_SERVER_BASE_URL_ENV)
    {
        findings.push(ServiceCheckFinding {
            level: "error",
            code: "common_server_base_url_env_not_supported",
            message: format!(
                "service `{}` defines env `{}`; ignis_login should not depend on COMMON_SERVER_BASE_URL as an env var",
                service.name, IGNIS_LOGIN_COMMON_SERVER_BASE_URL_ENV
            ),
        });
    }

    if service.ignis_login.is_some()
        && !service
            .network
            .allows_authority("cloud.transairobot.com", Some("cloud.transairobot.com"))
    {
        let message = if service.network.mode == NetworkMode::DenyAll {
            format!(
                "service `{}` configures `ignis_login` but `[services.network]` is `deny_all`; hosted login, token exchange, and userinfo need outbound access to `cloud.transairobot.com`",
                service.name
            )
        } else if service
            .network
            .allow
            .iter()
            .any(|entry| entry.eq_ignore_ascii_case("cloud.transairobot.com:443"))
        {
            format!(
                "service `{}` configures `ignis_login` but `[services.network].allow` only lists `cloud.transairobot.com:443`; add `cloud.transairobot.com` explicitly",
                service.name
            )
        } else {
            format!(
                "service `{}` configures `ignis_login` but `[services.network]` does not allow `cloud.transairobot.com`",
                service.name
            )
        };
        findings.push(ServiceCheckFinding {
            level: "error",
            code: "ignis_login_missing_common_server_allow",
            message,
        });
    }

    findings
}

fn ensure_service_check_passes(service: &ServiceManifest) -> Result<()> {
    let findings = collect_service_check_findings(service);
    let errors = findings
        .into_iter()
        .filter(|finding| finding.level == "error")
        .collect::<Vec<_>>();
    if errors.is_empty() {
        return Ok(());
    }

    let messages = errors
        .into_iter()
        .map(|finding| format!("[{}] {}", finding.code, finding.message))
        .collect::<Vec<_>>()
        .join("\n");
    bail!("service `{}` failed local checks:\n{messages}", service.name)
}

async fn delete_service(
    context: &ProjectContext,
    service: &str,
    token: Option<String>,
) -> Result<()> {
    ensure_service_exists(&context.loaded, service)?;
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client
        .delete_service(context.loaded.project_name(), service)
        .await?;

    let mut manifest = context.loaded.manifest.clone();
    manifest.services.retain(|item| item.name != service);
    save_project_manifest(&context.loaded.manifest_path, &manifest)?;

    print_json(&response)
}

async fn build_service(context: &ProjectContext, service: &str, release: bool) -> Result<()> {
    let service = required_service(&context.loaded, service)?;
    match service.kind {
        ServiceKind::Http => {
            let loaded = context.loaded.http_service_manifest(&service.name)?;
            build_http_service(&loaded, release).await
        }
        ServiceKind::Frontend => build_frontend_service(&context.loaded, service).await,
    }
}

async fn publish_service(
    context: &ProjectContext,
    service: &str,
    token: Option<String>,
) -> Result<()> {
    let service = required_service(&context.loaded, service)?;
    ensure_service_check_passes(service)?;
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = match service.kind {
        ServiceKind::Http => {
            let loaded = context.loaded.http_service_manifest(&service.name)?;
            let artifact_path = loaded.component_path();
            if !artifact_path.exists() {
                bail!(
                    "artifact {} does not exist; run `ignis service build --service {}` before publish",
                    artifact_path.display(),
                    service.name
                );
            }
            let component_signature = load_component_signature(&artifact_path)?;
            client
                .publish_http_service(
                    context.loaded.project_name(),
                    &service.name,
                    service,
                    &artifact_path,
                    component_signature,
                    build_metadata(&context.loaded, service).await?,
                )
                .await?
        }
        ServiceKind::Frontend => {
            let frontend = service.frontend.as_ref().ok_or_else(|| {
                anyhow!(
                    "frontend service `{}` is missing frontend config",
                    service.name
                )
            })?;
            let service_dir = context.loaded.service_dir(service);
            let output_dir = service_dir.join(&frontend.output_dir);
            if !output_dir.exists() {
                bail!(
                    "frontend output directory {} does not exist; run `ignis service build --service {}` before publish",
                    output_dir.display(),
                    service.name
                );
            }
            let bundle_path = create_tarball(&output_dir, &service.name).await?;
            let response = client
                .publish_frontend_service(
                    context.loaded.project_name(),
                    &service.name,
                    service,
                    &bundle_path,
                    build_metadata(&context.loaded, service).await?,
                )
                .await;
            let _ = fs::remove_file(&bundle_path);
            response?
        }
    };
    print_json(&response)
}

async fn deploy_service(
    context: &ProjectContext,
    service: &str,
    version: &str,
    token: Option<String>,
) -> Result<()> {
    ensure_service_exists(&context.loaded, service)?;
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client
        .deploy_service(context.loaded.project_name(), service, version)
        .await?;
    print_json(&response)
}

async fn deployments(
    context: &ProjectContext,
    service: &str,
    limit: u32,
    token: Option<String>,
) -> Result<()> {
    ensure_service_exists(&context.loaded, service)?;
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client
        .deployments(context.loaded.project_name(), service, limit)
        .await?;
    print_json(&response)
}

async fn events(
    context: &ProjectContext,
    service: &str,
    limit: u32,
    token: Option<String>,
) -> Result<()> {
    ensure_service_exists(&context.loaded, service)?;
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client
        .events(context.loaded.project_name(), service, limit)
        .await?;
    print_json(&response)
}

async fn logs(
    context: &ProjectContext,
    service: &str,
    limit: u32,
    token: Option<String>,
) -> Result<()> {
    ensure_service_exists(&context.loaded, service)?;
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client
        .logs(context.loaded.project_name(), service, limit)
        .await?;
    print_json(&response)
}

async fn rollback(
    context: &ProjectContext,
    service: &str,
    version: &str,
    token: Option<String>,
) -> Result<()> {
    ensure_service_exists(&context.loaded, service)?;
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client
        .rollback(context.loaded.project_name(), service, version)
        .await?;
    print_json(&response)
}

async fn delete_version(
    context: &ProjectContext,
    service: &str,
    version: &str,
    token: Option<String>,
) -> Result<()> {
    ensure_service_exists(&context.loaded, service)?;
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client
        .delete_version(context.loaded.project_name(), service, version)
        .await?;
    print_json(&response)
}

async fn env_command(
    context: &ProjectContext,
    command: ServiceEnvCommands,
    token: Option<String>,
) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = match command {
        ServiceEnvCommands::List { service } => {
            ensure_service_exists(&context.loaded, &service)?;
            client
                .env_list(context.loaded.project_name(), &service)
                .await?
        }
        ServiceEnvCommands::Set {
            service,
            name,
            value,
        } => {
            ensure_service_exists(&context.loaded, &service)?;
            client
                .env_set(context.loaded.project_name(), &service, &name, &value)
                .await?
        }
        ServiceEnvCommands::Delete { service, name } => {
            ensure_service_exists(&context.loaded, &service)?;
            client
                .env_delete(context.loaded.project_name(), &service, &name)
                .await?
        }
    };
    print_json(&response)
}

async fn secret_command(
    context: &ProjectContext,
    command: ServiceSecretCommands,
    token: Option<String>,
) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = match command {
        ServiceSecretCommands::List { service } => {
            ensure_service_exists(&context.loaded, &service)?;
            client
                .secrets_list(context.loaded.project_name(), &service)
                .await?
        }
        ServiceSecretCommands::Set {
            service,
            name,
            value,
        } => {
            ensure_service_exists(&context.loaded, &service)?;
            client
                .secrets_set(context.loaded.project_name(), &service, &name, &value)
                .await?
        }
        ServiceSecretCommands::Delete { service, name } => {
            ensure_service_exists(&context.loaded, &service)?;
            client
                .secrets_delete(context.loaded.project_name(), &service, &name)
                .await?
        }
    };
    print_json(&response)
}

async fn sqlite_command(
    context: &ProjectContext,
    command: ServiceSqliteCommands,
    token: Option<String>,
) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    match command {
        ServiceSqliteCommands::Backup { service, out } => {
            ensure_service_exists(&context.loaded, &service)?;
            let bytes = client
                .sqlite_backup(context.loaded.project_name(), &service)
                .await?;
            fs::write(&out, bytes).with_context(|| format!("writing {}", out.display()))?;
            info!(path = %out.display(), service = %service, "sqlite backup written");
            Ok(())
        }
        ServiceSqliteCommands::Restore { service, input } => {
            ensure_service_exists(&context.loaded, &service)?;
            let bytes = fs::read(&input).with_context(|| format!("reading {}", input.display()))?;
            let response = client
                .sqlite_restore(context.loaded.project_name(), &service, &bytes)
                .await?;
            print_json(&response)
        }
    }
}

fn load_project_context() -> Result<ProjectContext> {
    let manifest_path = find_project_manifest_path(std::env::current_dir()?)?;
    let loaded = LoadedProjectManifest::load(&manifest_path)?;
    Ok(ProjectContext { loaded })
}

fn find_project_manifest_path(start: PathBuf) -> Result<PathBuf> {
    let mut current = start;
    loop {
        let candidate = current.join(PROJECT_MANIFEST_FILE);
        if candidate.exists() {
            return Ok(candidate);
        }
        if !current.pop() {
            break;
        }
    }
    bail!(
        "could not find `{PROJECT_MANIFEST_FILE}` in the current directory or any parent directory"
    )
}

fn save_project_manifest(path: &Path, manifest: &ProjectManifest) -> Result<()> {
    fs::write(path, manifest.render()?).with_context(|| format!("writing {}", path.display()))
}

fn required_service<'a>(
    loaded: &'a LoadedProjectManifest,
    service_name: &str,
) -> Result<&'a ServiceManifest> {
    loaded.find_service(service_name).ok_or_else(|| {
        anyhow!(
            "service `{service_name}` not found in {}",
            loaded.manifest_path.display()
        )
    })
}

fn ensure_service_exists(loaded: &LoadedProjectManifest, service_name: &str) -> Result<()> {
    required_service(loaded, service_name).map(|_| ())
}

async fn build_http_service(loaded: &LoadedManifest, release: bool) -> Result<()> {
    if cargo_component_available().await? {
        run_command(
            &loaded.project_dir,
            "cargo",
            [
                "component",
                "build",
                if release { "--release" } else { "--debug" },
            ],
        )
        .await?;
    } else {
        ensure_rust_target("wasm32-wasip2").await?;
        let mut args = vec!["build", "--target", "wasm32-wasip2"];
        if release {
            args.push("--release");
        }
        run_command(&loaded.project_dir, "cargo", args).await?;
    }

    let output = loaded.component_path();
    if !output.exists() {
        bail!(
            "build finished but artifact was not found at {}",
            output.display()
        );
    }
    info!(artifact = %output.display(), "http service build completed");
    Ok(())
}

async fn build_frontend_service(
    loaded: &LoadedProjectManifest,
    service: &ServiceManifest,
) -> Result<()> {
    let frontend = service.frontend.as_ref().ok_or_else(|| {
        anyhow!(
            "frontend service `{}` is missing frontend config",
            service.name
        )
    })?;
    let service_dir = loaded.service_dir(service);
    let (program, args) = frontend.build_command.split_first().ok_or_else(|| {
        anyhow!(
            "frontend service `{}` build_command cannot be empty",
            service.name
        )
    })?;
    run_command(&service_dir, program, args.iter().map(String::as_str)).await?;
    let output_dir = service_dir.join(&frontend.output_dir);
    if !output_dir.exists() {
        bail!(
            "frontend build completed but output directory was not found at {}",
            output_dir.display()
        );
    }
    info!(path = %output_dir.display(), "frontend service build completed");
    Ok(())
}

async fn run_command<I, S>(cwd: &Path, program: &str, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let rendered_args: Vec<String> = args
        .into_iter()
        .map(|value| value.as_ref().to_owned())
        .collect();
    let status = Command::new(program)
        .args(&rendered_args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .with_context(|| format!("spawning `{program}` in {}", cwd.display()))?;
    if !status.success() {
        bail!("command `{program} {}` failed", rendered_args.join(" "));
    }
    Ok(())
}

async fn cargo_component_available() -> Result<bool> {
    let status = Command::new("cargo")
        .args(["component", "--version"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .context("checking cargo-component availability")?;
    Ok(status.success())
}

async fn ensure_rust_target(target: &str) -> Result<()> {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .await
        .context("checking installed rust targets")?;
    if !output.status.success() {
        bail!("`rustup target list --installed` failed; cannot verify Rust target `{target}`");
    }
    let installed = String::from_utf8_lossy(&output.stdout);
    if !installed.lines().any(|line| line.trim() == target) {
        bail!("Rust target `{target}` is not installed; run `rustup target add {target}` first");
    }
    Ok(())
}

async fn build_metadata(
    loaded: &LoadedProjectManifest,
    service: &ServiceManifest,
) -> Result<BTreeMap<String, String>> {
    let mut metadata = BTreeMap::new();
    metadata.insert("builder".to_owned(), "ignis-cli".to_owned());
    metadata.insert(
        "project_manifest_path".to_owned(),
        loaded.manifest_path.display().to_string(),
    );
    metadata.insert("project".to_owned(), loaded.project_name().to_owned());
    metadata.insert("service".to_owned(), service.name.clone());
    metadata.insert(
        "service_kind".to_owned(),
        match service.kind {
            ServiceKind::Http => "http".to_owned(),
            ServiceKind::Frontend => "frontend".to_owned(),
        },
    );
    metadata.insert(
        "service_path".to_owned(),
        loaded.service_dir(service).display().to_string(),
    );
    metadata.insert(
        "build_mode".to_owned(),
        match service.kind {
            ServiceKind::Http => {
                if cargo_component_available().await? {
                    "cargo-component".to_owned()
                } else {
                    "cargo-build-wasm32-wasip2".to_owned()
                }
            }
            ServiceKind::Frontend => "frontend-build-command".to_owned(),
        },
    );
    Ok(metadata)
}

async fn create_tarball(output_dir: &Path, service_name: &str) -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let bundle_name = format!("ignis-{service_name}-{nanos}.tar.gz");
    let bundle_path = std::env::temp_dir().join(bundle_name);
    run_command(
        output_dir,
        "tar",
        [
            "-czf",
            bundle_path
                .to_str()
                .ok_or_else(|| anyhow!("temporary tarball path is not valid UTF-8"))?,
            ".",
        ],
    )
    .await?;
    Ok(bundle_path)
}

async fn login(token: Option<String>) -> Result<()> {
    if token.is_some() {
        bail!("`ignis login` now uses browser sign-in; do not pass `--token`");
    }

    let state = new_login_state();
    let (redirect_uri, receiver, handle) = start_loopback_login_listener(state.clone())?;
    let login_url = build_browser_login_url(&redirect_uri, &state)?;

    eprintln!("Opening browser for igniscloud login...");
    if !open_browser(&login_url) {
        eprintln!("Open this URL in your browser:\n{login_url}");
    }

    let payload = tokio::task::spawn_blocking(move || receiver.recv_timeout(LOGIN_TIMEOUT))
        .await
        .context("waiting for browser login task failed")?
        .map_err(|error| anyhow!("timed out waiting for browser login: {error}"))??;

    handle
        .join()
        .map_err(|_| anyhow!("loopback login listener thread panicked"))?;

    let mut config = config::CliConfig::load()?.unwrap_or(config::CliConfig {
        server: config::DEFAULT_SERVER.to_owned(),
        token: String::new(),
        user_sub: None,
        user_aud: None,
        user_display_name: None,
    });
    config.server = config::DEFAULT_SERVER.to_owned();
    config.token = payload.token;
    config.user_sub = payload.user_sub;
    config.user_aud = payload.user_aud;
    config.user_display_name = payload.user_display_name;
    let path = config.save()?;
    eprintln!("Saved login to {}", path.display());
    println!("Login successful");
    Ok(())
}

fn logout() -> Result<()> {
    match config::CliConfig::clear()? {
        Some(path) => {
            eprintln!("Removed login at {}", path.display());
            Ok(())
        }
        None => {
            eprintln!(
                "No saved login found at {}",
                config::default_config_path().display()
            );
            Ok(())
        }
    }
}

async fn whoami(token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.whoami().await?;
    print_json(&response)
}

#[derive(Debug)]
struct LoopbackLoginPayload {
    token: String,
    user_sub: Option<String>,
    user_aud: Option<String>,
    user_display_name: Option<String>,
}

fn build_browser_login_url(redirect_uri: &str, state: &str) -> Result<String> {
    let mut url = reqwest::Url::parse(&format!(
        "{}/v1/cli/auth/start",
        config::DEFAULT_SERVER.trim_end_matches('/')
    ))?;
    url.query_pairs_mut()
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", state);
    Ok(url.to_string())
}

fn start_loopback_login_listener(
    expected_state: String,
) -> Result<(
    String,
    mpsc::Receiver<Result<LoopbackLoginPayload>>,
    thread::JoinHandle<()>,
)> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .context("binding localhost callback server failed")?;
    let port = listener
        .local_addr()
        .context("reading localhost callback address failed")?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let result = match listener.accept() {
            Ok((mut stream, _)) => handle_loopback_login_request(&mut stream, &expected_state),
            Err(error) => Err(anyhow!("accepting localhost callback failed: {error}")),
        };
        let _ = sender.send(result);
    });
    Ok((redirect_uri, receiver, handle))
}

fn handle_loopback_login_request(
    stream: &mut std::net::TcpStream,
    expected_state: &str,
) -> Result<LoopbackLoginPayload> {
    let request = read_http_request(stream)?;
    let (method, path) = parse_request_line(&request.headers)?;
    let form = if method == "GET" {
        parse_query_string(&path)?
    } else if method == "POST" {
        parse_form_body(&request.body)?
    } else {
        write_http_html_response(
            stream,
            "405 Method Not Allowed",
            "<h1>Method Not Allowed</h1><p>Ignis CLI expects a browser redirect to localhost.</p>",
        )?;
        bail!("unexpected callback method `{method}`");
    };
    if !path.starts_with("/callback") {
        write_http_html_response(
            stream,
            "404 Not Found",
            "<h1>Not Found</h1><p>Unknown Ignis CLI callback path.</p>",
        )?;
        bail!("unexpected callback path `{path}`");
    }
    let state = form
        .get("state")
        .ok_or_else(|| anyhow!("login callback is missing state"))?;
    if state != expected_state {
        write_http_html_response(
            stream,
            "400 Bad Request",
            "<h1>Login Failed</h1><p>State verification failed. Return to the terminal and retry.</p>",
        )?;
        bail!("login callback state mismatch");
    }
    let token = form
        .get("token")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("login callback is missing token"))?;

    write_http_html_response(
        stream,
        "200 OK",
        "<!doctype html><html><body><h1>Login successful</h1><p>You can close this window and return to Ignis CLI.</p><script>window.close();</script></body></html>",
    )?;

    Ok(LoopbackLoginPayload {
        token,
        user_sub: form
            .get("user_sub")
            .cloned()
            .filter(|value| !value.is_empty()),
        user_aud: form
            .get("user_aud")
            .cloned()
            .filter(|value| !value.is_empty()),
        user_display_name: form
            .get("user_display_name")
            .cloned()
            .filter(|value| !value.is_empty()),
    })
}

struct HttpRequest {
    headers: String,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut std::net::TcpStream) -> Result<HttpRequest> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        let read = stream
            .read(&mut chunk)
            .context("reading localhost callback failed")?;
        if read == 0 {
            bail!("localhost callback closed before sending headers");
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(index) = find_header_end(&buffer) {
            break index;
        }
    };
    let headers = String::from_utf8(buffer[..header_end].to_vec())
        .context("localhost callback headers are not valid UTF-8")?;
    let content_length = parse_content_length(&headers)?;
    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let read = stream
            .read(&mut chunk)
            .context("reading localhost callback body failed")?;
        if read == 0 {
            bail!("localhost callback closed before sending full body");
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    Ok(HttpRequest {
        headers,
        body: buffer[body_start..body_start + content_length].to_vec(),
    })
}

fn parse_request_line(headers: &str) -> Result<(String, String)> {
    let line = headers
        .lines()
        .next()
        .ok_or_else(|| anyhow!("localhost callback request line is missing"))?;
    let mut parts = line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| anyhow!("localhost callback method is missing"))?;
    let path = parts
        .next()
        .ok_or_else(|| anyhow!("localhost callback path is missing"))?;
    Ok((method.to_owned(), path.to_owned()))
}

fn parse_content_length(headers: &str) -> Result<usize> {
    for line in headers.lines() {
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            return value
                .trim()
                .parse::<usize>()
                .context("invalid callback content-length");
        }
    }
    Ok(0)
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_form_body(body: &[u8]) -> Result<BTreeMap<String, String>> {
    let text = String::from_utf8(body.to_vec()).context("callback form body is not valid UTF-8")?;
    parse_form_encoded_values(&text)
}

fn parse_query_string(path: &str) -> Result<BTreeMap<String, String>> {
    let Some((_, query)) = path.split_once('?') else {
        return Ok(BTreeMap::new());
    };
    parse_form_encoded_values(query)
}

fn parse_form_encoded_values(text: &str) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    for pair in text.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        values.insert(percent_decode(name)?, percent_decode(value)?);
    }
    Ok(values)
}

fn percent_decode(value: &str) -> Result<String> {
    let mut output = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' => {
                if index + 2 >= bytes.len() {
                    bail!("invalid percent-encoded callback data");
                }
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                    .context("callback form contains invalid percent-encoding")?;
                let byte = u8::from_str_radix(hex, 16)
                    .context("callback form contains invalid percent-encoding")?;
                output.push(byte);
                index += 3;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(output)
        .context("callback form contains invalid UTF-8")
        .map_err(Into::into)
}

fn write_http_html_response(
    stream: &mut std::net::TcpStream,
    status: &str,
    body: &str,
) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .context("writing localhost callback response failed")
}

fn new_login_state() -> String {
    format!(
        "ignis-login-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default()
    )
}

fn open_browser(url: &str) -> bool {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).status()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .status()
    } else {
        std::process::Command::new("xdg-open").arg(url).status()
    };
    result.map(|status| status.success()).unwrap_or(false)
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn load_component_signature(artifact_path: &Path) -> Result<Option<ComponentSignature>> {
    let key_id = std::env::var("IGNIS_SIGNING_KEY_ID").ok();
    let key_seed = std::env::var("IGNIS_SIGNING_KEY_BASE64").ok();
    match (key_id, key_seed) {
        (Some(key_id), Some(key_seed)) => {
            let component = fs::read(artifact_path)
                .with_context(|| format!("reading {}", artifact_path.display()))?;
            Ok(Some(sign_component_with_seed(
                &component, &key_id, &key_seed,
            )?))
        }
        (None, None) => Ok(None),
        _ => bail!(
            "set both IGNIS_SIGNING_KEY_ID and IGNIS_SIGNING_KEY_BASE64 to publish a signed component"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{SkillFormat, collect_service_check_findings, gen_skill_output_root};
    use ignis_manifest::{
        HttpServiceConfig, IgnisLoginConfig, IgnisLoginProvider, NetworkConfig, NetworkMode,
        ResourceConfig, ServiceKind, ServiceManifest, SqliteConfig,
    };
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn gen_skill_output_root_uses_format_defaults() {
        assert_eq!(
            gen_skill_output_root(SkillFormat::Codex, None),
            Path::new(".codex").join("skills").join("ignis-user")
        );
        assert_eq!(
            gen_skill_output_root(SkillFormat::Opencode, None),
            Path::new(".opencode").join("skills").join("ignis-user")
        );
        assert_eq!(
            gen_skill_output_root(SkillFormat::Raw, None),
            Path::new("ignis-user")
        );
    }

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
            network: NetworkConfig {
                mode: NetworkMode::AllowList,
                allow: vec!["cloud.transairobot.com".to_owned()],
            },
        }
    }

    #[test]
    fn service_check_flags_common_server_base_url_env() {
        let mut service = sample_http_service();
        service.env.insert(
            "COMMON_SERVER_BASE_URL".to_owned(),
            "https://cloud.transairobot.com".to_owned(),
        );

        let findings = collect_service_check_findings(&service);

        assert!(findings
            .iter()
            .any(|finding| finding.code == "common_server_base_url_env_not_supported"));
    }

    #[test]
    fn service_check_flags_ignis_login_allow_rule_with_only_port_specific_host() {
        let mut service = sample_http_service();
        service.network.allow = vec!["cloud.transairobot.com:443".to_owned()];

        let findings = collect_service_check_findings(&service);

        assert!(findings
            .iter()
            .any(|finding| finding.code == "ignis_login_missing_common_server_allow"));
    }

    #[test]
    fn service_check_accepts_ignis_login_host_allow_rule() {
        let service = sample_http_service();

        let findings = collect_service_check_findings(&service);

        assert!(findings.is_empty());
    }
}
