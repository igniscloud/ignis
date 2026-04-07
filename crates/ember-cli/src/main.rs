mod api;
mod config;
mod template;

use std::collections::BTreeMap;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use ember_manifest::{
    ComponentSignature, LoadedManifest, MANIFEST_FILE, NetworkConfig, ResourceConfig, SqliteConfig,
    WorkerManifest, sign_component_with_seed,
};
use ember_runtime::DevServerConfig;
use serde_json::Value;
use tokio::process::Command;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "ember", version, about = "Ember worker CLI")]
struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "TOKEN",
        help = "API token or app token for embercloud; also supports EMBER_TOKEN, EMBERCLOUD_TOKEN, or WKR_API_TOKEN"
    )]
    token: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init {
        path: PathBuf,
        #[arg(long)]
        force: bool,
    },
    Whoami,
    Build {
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long, default_value_t = true)]
        release: bool,
    },
    Dev {
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1:3000")]
        addr: SocketAddr,
        #[arg(long)]
        skip_build: bool,
    },
    Publish {
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
    Deploy {
        app: String,
        version: String,
    },
    Status {
        app: String,
    },
    Apps,
    Nodes,
    Deployments {
        app: String,
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    Events {
        app: String,
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    Env {
        #[command(subcommand)]
        command: EnvCommands,
    },
    Secrets {
        #[command(subcommand)]
        command: SecretCommands,
    },
    Logs {
        app: String,
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    Rollback {
        app: String,
        version: String,
    },
    DeleteVersion {
        app: String,
        version: String,
    },
    DeleteApp {
        app: String,
    },
    Sqlite {
        #[command(subcommand)]
        command: SqliteCommands,
    },
}

#[derive(Debug, Subcommand)]
enum EnvCommands {
    List {
        app: String,
    },
    Set {
        app: String,
        name: String,
        value: String,
    },
    Delete {
        app: String,
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum SecretCommands {
    List {
        app: String,
    },
    Set {
        app: String,
        name: String,
        value: String,
    },
    Delete {
        app: String,
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum SqliteCommands {
    Backup { app: String, out: PathBuf },
    Restore { app: String, input: PathBuf },
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
        Commands::Init { path, force } => init_project(&path, force),
        Commands::Whoami => whoami(token).await,
        Commands::Build { manifest, release } => {
            let loaded = load_manifest(manifest)?;
            build_project(&loaded, release).await
        }
        Commands::Dev {
            manifest,
            addr,
            skip_build,
        } => {
            let loaded = load_manifest(manifest)?;
            if !skip_build {
                build_project(&loaded, true).await?;
            }
            ember_runtime::serve(loaded, DevServerConfig { listen_addr: addr }).await
        }
        Commands::Publish { manifest } => {
            let loaded = load_manifest(manifest)?;
            publish(&loaded, token).await
        }
        Commands::Deploy { app, version } => deploy(&app, &version, token).await,
        Commands::Status { app } => status(&app, token).await,
        Commands::Apps => list_apps(token).await,
        Commands::Nodes => list_nodes(token).await,
        Commands::Deployments { app, limit } => deployments(&app, limit, token).await,
        Commands::Events { app, limit } => events(&app, limit, token).await,
        Commands::Env { command } => env_command(command, token).await,
        Commands::Secrets { command } => secret_command(command, token).await,
        Commands::Logs { app, limit } => logs(&app, limit, token).await,
        Commands::Rollback { app, version } => rollback(&app, &version, token).await,
        Commands::DeleteVersion { app, version } => delete_version(&app, &version, token).await,
        Commands::DeleteApp { app } => delete_app(&app, token).await,
        Commands::Sqlite { command } => sqlite_command(command, token).await,
    }
}

fn init_project(path: &Path, force: bool) -> Result<()> {
    if path.exists() {
        let mut entries = path
            .read_dir()
            .with_context(|| format!("reading {}", path.display()))?;
        if entries.next().is_some() && !force {
            bail!(
                "directory {} is not empty; pass --force to overwrite template files",
                path.display()
            );
        }
    }

    let package_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("cannot derive package name from {}", path.display()))?
        .replace('-', "_");
    let display_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("cannot derive application name from {}", path.display()))?;
    fs::create_dir_all(path.join("src"))
        .with_context(|| format!("creating {}", path.join("src").display()))?;
    fs::create_dir_all(path.join("wit"))
        .with_context(|| format!("creating {}", path.join("wit").display()))?;

    let manifest = WorkerManifest {
        name: display_name.to_owned(),
        component: PathBuf::from(format!("target/wasm32-wasip2/release/{package_name}.wasm")),
        base_path: "/".to_owned(),
        env: Default::default(),
        secrets: Default::default(),
        sqlite: SqliteConfig { enabled: true },
        resources: ResourceConfig {
            cpu_time_limit_ms: Some(5_000),
            memory_limit_bytes: Some(128 * 1024 * 1024),
        },
        network: NetworkConfig::default(),
    };

    fs::write(path.join("Cargo.toml"), template::cargo_toml(display_name))
        .with_context(|| format!("writing {}", path.join("Cargo.toml").display()))?;
    fs::write(path.join("src/lib.rs"), template::lib_rs())
        .with_context(|| format!("writing {}", path.join("src/lib.rs").display()))?;
    fs::write(
        path.join("wit/world.wit"),
        template::world_wit(display_name),
    )
    .with_context(|| format!("writing {}", path.join("wit/world.wit").display()))?;
    fs::write(path.join(MANIFEST_FILE), manifest.render()?)
        .with_context(|| format!("writing {}", path.join(MANIFEST_FILE).display()))?;
    fs::write(path.join(".gitignore"), template::gitignore())
        .with_context(|| format!("writing {}", path.join(".gitignore").display()))?;

    info!(path = %path.display(), "initialized worker project");
    Ok(())
}

async fn whoami(token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.whoami().await?;
    print_json(&response)
}

fn load_manifest(path: Option<PathBuf>) -> Result<LoadedManifest> {
    let manifest_path = path.unwrap_or_else(|| PathBuf::from("."));
    LoadedManifest::load(&manifest_path)
}

async fn build_project(loaded: &LoadedManifest, release: bool) -> Result<()> {
    if cargo_component_available().await? {
        run_build_command(
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
        run_build_command(&loaded.project_dir, "cargo", args).await?;
    }

    let output = loaded.component_path();
    if !output.exists() {
        bail!(
            "build finished but artifact was not found at {}",
            output.display()
        );
    }
    info!(artifact = %output.display(), "build completed");
    Ok(())
}

async fn run_build_command<I, S>(cwd: &Path, program: &str, args: I) -> Result<()>
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
        bail!(
            "build command `{program} {}` failed",
            rendered_args.join(" ")
        );
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

async fn publish(loaded: &LoadedManifest, token: Option<String>) -> Result<()> {
    let config = config::CliConfig::resolve(token)?;
    let client = api::ApiClient::new(config);
    let artifact_path = loaded.component_path();
    if !artifact_path.exists() {
        bail!(
            "artifact {} does not exist; run `ember build` before publish",
            artifact_path.display()
        );
    }
    let component_signature = load_component_signature(&artifact_path)?;
    let response = client
        .publish(
            loaded,
            &artifact_path,
            component_signature,
            build_metadata(loaded).await?,
        )
        .await?;
    print_json(&response)
}

async fn deploy(app: &str, version: &str, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.deploy(app, version).await?;
    print_json(&response)
}

async fn status(app: &str, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.status(app).await?;
    print_status_summary(&response);
    print_json(&response)
}

async fn list_apps(token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.apps().await?;
    print_app_list_summary(&response);
    print_json(&response)
}

async fn list_nodes(token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.nodes().await?;
    print_json(&response)
}

async fn deployments(app: &str, limit: u32, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.deployments(app, limit).await?;
    print_json(&response)
}

async fn events(app: &str, limit: u32, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.events(app, limit).await?;
    print_json(&response)
}

async fn env_command(command: EnvCommands, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = match command {
        EnvCommands::List { app } => client.env_list(&app).await?,
        EnvCommands::Set { app, name, value } => client.env_set(&app, &name, &value).await?,
        EnvCommands::Delete { app, name } => client.env_delete(&app, &name).await?,
    };
    print_json(&response)
}

async fn secret_command(command: SecretCommands, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = match command {
        SecretCommands::List { app } => client.secrets_list(&app).await?,
        SecretCommands::Set { app, name, value } => client.secrets_set(&app, &name, &value).await?,
        SecretCommands::Delete { app, name } => client.secrets_delete(&app, &name).await?,
    };
    print_json(&response)
}

async fn logs(app: &str, limit: u32, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.logs(app, limit).await?;
    print_json(&response)
}

async fn rollback(app: &str, version: &str, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.rollback(app, version).await?;
    print_json(&response)
}

async fn delete_version(app: &str, version: &str, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.delete_version(app, version).await?;
    print_json(&response)
}

async fn delete_app(app: &str, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.delete_app(app).await?;
    print_json(&response)
}

async fn sqlite_command(command: SqliteCommands, token: Option<String>) -> Result<()> {
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    match command {
        SqliteCommands::Backup { app, out } => {
            let bytes = client.sqlite_backup(&app).await?;
            fs::write(&out, bytes).with_context(|| format!("writing {}", out.display()))?;
            info!(path = %out.display(), app = %app, "sqlite backup written");
            Ok(())
        }
        SqliteCommands::Restore { app, input } => {
            let bytes = fs::read(&input).with_context(|| format!("reading {}", input.display()))?;
            let response = client.sqlite_restore(&app, &bytes).await?;
            print_json(&response)
        }
    }
}

async fn build_metadata(loaded: &LoadedManifest) -> Result<BTreeMap<String, String>> {
    let mut metadata = BTreeMap::new();
    metadata.insert("builder".to_owned(), "ember-cli".to_owned());
    metadata.insert(
        "manifest_path".to_owned(),
        loaded.manifest_path.display().to_string(),
    );
    metadata.insert(
        "component_path".to_owned(),
        loaded.component_path().display().to_string(),
    );
    metadata.insert(
        "build_mode".to_owned(),
        if cargo_component_available().await? {
            "cargo-component".to_owned()
        } else {
            "cargo-build-wasm32-wasip2".to_owned()
        },
    );
    Ok(metadata)
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_status_summary(value: &Value) {
    let Some(data) = value.get("data") else {
        return;
    };
    if let Some(url) = json_string(data, "access_url") {
        eprintln!("URL: {url}");
    } else if let Some(host) = json_string(data, "access_host") {
        eprintln!("Host: {host}");
    }
}

fn print_app_list_summary(value: &Value) {
    let Some(items) = value.get("data").and_then(Value::as_array) else {
        return;
    };
    let summaries = items
        .iter()
        .filter_map(|item| {
            let app = json_string(item, "app")?;
            if let Some(url) = json_string(item, "access_url") {
                Some(format!("{app}: {url}"))
            } else {
                json_string(item, "access_host").map(|host| format!("{app}: {host}"))
            }
        })
        .collect::<Vec<_>>();
    if summaries.is_empty() {
        return;
    }
    eprintln!("Access URLs:");
    for summary in summaries {
        eprintln!("  {summary}");
    }
}

fn json_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|item| !item.is_empty())
}

fn load_component_signature(artifact_path: &Path) -> Result<Option<ComponentSignature>> {
    let key_id = std::env::var("EMBER_SIGNING_KEY_ID")
        .ok()
        .or_else(|| std::env::var("WKR_SIGNING_KEY_ID").ok());
    let key_seed = std::env::var("EMBER_SIGNING_KEY_BASE64")
        .ok()
        .or_else(|| std::env::var("WKR_SIGNING_KEY_BASE64").ok());
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
            "set both EMBER_SIGNING_KEY_ID and EMBER_SIGNING_KEY_BASE64 to publish a signed component"
        ),
    }
}
