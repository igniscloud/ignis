mod api;
mod config;
mod template;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use ember_manifest::{
    ComponentSignature, EmberCloudConfig, LoadedManifest, MANIFEST_FILE, NetworkConfig,
    ResourceConfig, SqliteConfig, WorkerManifest, sign_component_with_seed,
};
use ember_runtime::DevServerConfig;
use serde_json::Value;
use tokio::process::Command;
use tracing::info;
use tracing_subscriber::EnvFilter;

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Parser)]
#[command(name = "ember", version, about = "Ember worker CLI")]
struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "TOKEN",
        help = "Login access token, API token, or app token for embercloud; also supports EMBER_TOKEN, EMBERCLOUD_TOKEN, or WKR_API_TOKEN"
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
    Login,
    Logout,
    Whoami,
    Build {
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long, default_value_t = true)]
        release: bool,
    },
    CreateApp {
        app: Option<String>,
        #[arg(long)]
        manifest: Option<PathBuf>,
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
        #[arg(long)]
        app: Option<String>,
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(value_name = "APP_OR_VERSION", num_args = 1..=2)]
        args: Vec<String>,
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
        Commands::Login => login(token).await,
        Commands::Logout => logout(),
        Commands::Whoami => whoami(token).await,
        Commands::Build { manifest, release } => {
            let loaded = load_manifest(manifest)?;
            build_project(&loaded, release).await
        }
        Commands::CreateApp { app, manifest } => create_app_command(app, manifest, token).await,
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
        Commands::Deploy {
            app,
            manifest,
            args,
        } => {
            let (app, version) = parse_deploy_args(app, args)?;
            deploy(app, manifest, &version, token).await
        }
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
        embercloud: EmberCloudConfig {
            app: Some(display_name.to_owned()),
        },
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

async fn login(token: Option<String>) -> Result<()> {
    if token.is_some() {
        bail!("`ember login` now uses browser sign-in; do not pass `--token`");
    }

    let state = new_login_state();
    let (redirect_uri, receiver, handle) = start_loopback_login_listener(state.clone())?;
    let login_url = build_browser_login_url(&redirect_uri, &state)?;

    eprintln!("Opening browser for embercloud login...");
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

async fn create_app_command(
    app: Option<String>,
    manifest: Option<PathBuf>,
    token: Option<String>,
) -> Result<()> {
    let loaded = if app.is_some() {
        load_manifest_optional(manifest)?
    } else {
        Some(load_manifest(manifest)?)
    };
    let app_name = resolve_target_app(app, loaded.as_ref())?;
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    let response = client.create_app(&app_name).await?;
    print_json(&response)
}

async fn publish(loaded: &LoadedManifest, token: Option<String>) -> Result<()> {
    let config = config::CliConfig::resolve(token)?;
    let client = api::ApiClient::new(config);
    let app = required_manifest_app(loaded)?;
    let artifact_path = loaded.component_path();
    if !artifact_path.exists() {
        bail!(
            "artifact {} does not exist; run `ember build` before publish",
            artifact_path.display()
        );
    }
    ensure_app_exists(&client, app, true).await?;
    let component_signature = load_component_signature(&artifact_path)?;
    let response = client
        .publish(
            app,
            loaded,
            &artifact_path,
            component_signature,
            build_metadata(loaded).await?,
        )
        .await?;
    print_json(&response)
}

async fn deploy(
    app: Option<String>,
    manifest: Option<PathBuf>,
    version: &str,
    token: Option<String>,
) -> Result<()> {
    let loaded = if manifest.is_some() || app.is_none() {
        Some(load_manifest(manifest)?)
    } else {
        None
    };
    let app = resolve_target_app(app, loaded.as_ref())?;
    let client = api::ApiClient::new(config::CliConfig::resolve(token)?);
    ensure_app_exists(&client, &app, true).await?;
    let response = client.deploy(&app, version).await?;
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

fn parse_deploy_args(app: Option<String>, args: Vec<String>) -> Result<(Option<String>, String)> {
    match args.as_slice() {
        [version] => Ok((app, version.clone())),
        [legacy_app, version] => {
            if app.is_some() {
                bail!("pass the app name either as `--app` or as the first positional argument");
            }
            Ok((Some(legacy_app.clone()), version.clone()))
        }
        _ => bail!("deploy expects `<version>` or `<app> <version>`"),
    }
}

fn load_manifest_optional(path: Option<PathBuf>) -> Result<Option<LoadedManifest>> {
    let Some(path) = path else {
        let default_path = PathBuf::from(MANIFEST_FILE);
        if !default_path.exists() {
            return Ok(None);
        }
        return LoadedManifest::load(default_path).map(Some);
    };
    LoadedManifest::load(path).map(Some)
}

fn resolve_target_app(app: Option<String>, loaded: Option<&LoadedManifest>) -> Result<String> {
    let cli_app = app
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let manifest_app = loaded.and_then(LoadedManifest::embercloud_app);
    if let (Some(cli_app), Some(manifest_app)) = (cli_app.as_deref(), manifest_app) {
        if cli_app != manifest_app {
            bail!("CLI app `{cli_app}` does not match worker.toml embercloud.app `{manifest_app}`");
        }
    }
    if let Some(cli_app) = cli_app {
        return Ok(cli_app);
    }
    if let Some(manifest_app) = manifest_app {
        return Ok(manifest_app.to_owned());
    }
    bail!("missing cloud app; set `[embercloud] app = \"your-app\"` in worker.toml or pass `--app`")
}

fn required_manifest_app(loaded: &LoadedManifest) -> Result<&str> {
    loaded.embercloud_app().ok_or_else(|| {
        anyhow!(
            "missing `[embercloud] app` in {}",
            loaded.manifest_path.display()
        )
    })
}

async fn ensure_app_exists(client: &api::ApiClient, app: &str, interactive: bool) -> Result<()> {
    if app_list_contains(&client.apps().await?, app) {
        return Ok(());
    }
    if !interactive || !io::stdin().is_terminal() {
        bail!(
            "cloud app `{app}` does not exist; create it with `ember create-app --app {app}` or update worker.toml"
        );
    }
    if !prompt_yes_no(&format!(
        "Cloud app `{app}` does not exist. Create it now? [y/N]: "
    ))? {
        bail!("cloud app `{app}` does not exist");
    }
    let response = client.create_app(app).await?;
    let created_app = response
        .get("data")
        .and_then(|data| data.get("app"))
        .and_then(Value::as_str)
        .unwrap_or(app);
    eprintln!("Created cloud app {created_app}");
    Ok(())
}

fn app_list_contains(value: &Value, app: &str) -> bool {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items.iter().any(|item| {
                item.get("app")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value == app)
            })
        })
        .unwrap_or(false)
}

fn prompt_yes_no(message: &str) -> Result<bool> {
    let mut stderr = io::stderr();
    stderr.write_all(message.as_bytes())?;
    stderr.flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim().to_ascii_lowercase();
    Ok(matches!(answer.as_str(), "y" | "yes"))
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
            "<h1>Method Not Allowed</h1><p>Ember CLI expects a browser redirect to localhost.</p>",
        )?;
        bail!("unexpected callback method `{method}`");
    };
    if !path.starts_with("/callback") {
        write_http_html_response(
            stream,
            "404 Not Found",
            "<h1>Not Found</h1><p>Unknown Ember CLI callback path.</p>",
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
        "<!doctype html><html><body><h1>Login successful</h1><p>You can close this window and return to Ember CLI.</p><script>window.close();</script></body></html>",
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
        "ember-login-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
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
