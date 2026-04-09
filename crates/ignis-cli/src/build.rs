use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use ignis_manifest::{ComponentSignature, LoadedManifest, ServiceKind, sign_component_with_seed};
use serde::Serialize;
use tokio::process::Command;
use tracing::info;

use crate::context::ServiceContext;

#[derive(Debug, Clone, Serialize)]
pub struct BuildOutcome {
    pub mode: &'static str,
    pub output_path: PathBuf,
}

pub async fn build_service(service: &ServiceContext<'_>, release: bool) -> Result<BuildOutcome> {
    match service.manifest().kind {
        ServiceKind::Http => {
            let loaded = service.http_service_manifest()?;
            let output_path = build_http_service(&loaded, release).await?;
            Ok(BuildOutcome {
                mode: "cargo-build-wasm32-wasip2",
                output_path,
            })
        }
        ServiceKind::Frontend => {
            let output_path = build_frontend_service(service).await?;
            Ok(BuildOutcome {
                mode: "frontend-build-command",
                output_path,
            })
        }
    }
}

pub async fn build_metadata(service: &ServiceContext<'_>) -> Result<BTreeMap<String, String>> {
    let mut metadata = BTreeMap::new();
    metadata.insert("builder".to_owned(), "ignis-cli".to_owned());
    metadata.insert(
        "project_manifest_path".to_owned(),
        service.project().manifest_path().display().to_string(),
    );
    metadata.insert("project".to_owned(), service.project_name().to_owned());
    metadata.insert("service".to_owned(), service.name().to_owned());
    metadata.insert(
        "service_kind".to_owned(),
        kind_name(service.manifest().kind).to_owned(),
    );
    metadata.insert(
        "service_path".to_owned(),
        service.service_dir().display().to_string(),
    );
    metadata.insert(
        "build_mode".to_owned(),
        match service.manifest().kind {
            ServiceKind::Http => "cargo-build-wasm32-wasip2".to_owned(),
            ServiceKind::Frontend => "frontend-build-command".to_owned(),
        },
    );
    Ok(metadata)
}

pub async fn create_frontend_bundle(service: &ServiceContext<'_>) -> Result<PathBuf> {
    let frontend = service.manifest().frontend.as_ref().ok_or_else(|| {
        anyhow!(
            "frontend service `{}` is missing frontend config",
            service.name()
        )
    })?;
    let output_dir = service.service_dir().join(&frontend.output_dir);
    if !output_dir.exists() {
        bail!(
            "frontend output directory {} does not exist; run `ignis service build --service {}` before publish",
            output_dir.display(),
            service.name()
        );
    }
    create_tarball(&output_dir, service.name()).await
}

pub fn load_component_signature(artifact_path: &Path) -> Result<Option<ComponentSignature>> {
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

async fn build_http_service(loaded: &LoadedManifest, release: bool) -> Result<PathBuf> {
    ensure_rust_target("wasm32-wasip2").await?;
    let mut args = vec!["build", "--target", "wasm32-wasip2"];
    if release {
        args.push("--release");
    }
    run_command(&loaded.project_dir, "cargo", args).await?;

    let output = loaded.component_path();
    if !output.exists() {
        bail!(
            "build finished but artifact was not found at {}",
            output.display()
        );
    }
    info!(artifact = %output.display(), "http service build completed");
    Ok(output)
}

async fn build_frontend_service(service: &ServiceContext<'_>) -> Result<PathBuf> {
    let frontend = service.manifest().frontend.as_ref().ok_or_else(|| {
        anyhow!(
            "frontend service `{}` is missing frontend config",
            service.name()
        )
    })?;
    let service_dir = service.service_dir();
    let (program, args) = frontend.build_command.split_first().ok_or_else(|| {
        anyhow!(
            "frontend service `{}` build_command cannot be empty",
            service.name()
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
    Ok(output_dir)
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

fn kind_name(kind: ServiceKind) -> &'static str {
    match kind {
        ServiceKind::Http => "http",
        ServiceKind::Frontend => "frontend",
    }
}
