use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use flate2::Compression;
use flate2::write::GzEncoder;
use ignis_manifest::{
    AgentRuntime, ComponentSignature, LoadedManifest, PUBLISHED_SERVICE_PLAN_BUILD_METADATA_KEY,
    ServiceKind, effective_agent_service_config, sign_component_with_seed,
};
use serde::Serialize;
use tar::Builder;
use tokio::process::Command;
use tracing::info;

use crate::cli::InternalCommands;
use crate::context::ServiceContext;

#[derive(Debug, Clone, Serialize)]
pub struct BuildOutcome {
    pub mode: &'static str,
    pub output_path: PathBuf,
    pub validation: ArtifactValidation,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactValidation {
    pub kind: &'static str,
    pub artifact_path: PathBuf,
    pub checks: Vec<ValidationCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationCheck {
    pub name: &'static str,
    pub detail: String,
}

#[derive(Debug)]
pub struct PublishArtifact {
    pub metadata: BTreeMap<String, String>,
    pub validation: ArtifactValidation,
    pub kind: PublishArtifactKind,
}

#[derive(Debug)]
pub enum PublishArtifactKind {
    Http {
        component_path: PathBuf,
        component_signature: Option<ComponentSignature>,
    },
    Frontend {
        bundle_path: PathBuf,
    },
    Agent {
        bundle_path: PathBuf,
    },
}

impl PublishArtifact {
    pub fn cleanup(self) {
        match self.kind {
            PublishArtifactKind::Frontend { bundle_path }
            | PublishArtifactKind::Agent { bundle_path } => {
                let _ = fs::remove_file(bundle_path);
            }
            PublishArtifactKind::Http { .. } => {}
        }
    }
}

pub async fn handle_internal(command: InternalCommands) -> Result<()> {
    match command {
        InternalCommands::CopyFrontendStatic {
            source_dir,
            output_dir,
        } => copy_frontend_static_site(
            &std::env::current_dir().context("reading current directory failed")?,
            &source_dir,
            &output_dir,
        ),
    }
}

pub async fn build_service(service: &ServiceContext<'_>, release: bool) -> Result<BuildOutcome> {
    match service.manifest().kind {
        ServiceKind::Http => {
            let loaded = service.http_service_manifest()?;
            let output_path = build_http_service(&loaded, release).await?;
            let validation = validate_http_artifact(&loaded)?;
            Ok(BuildOutcome {
                mode: "cargo-build-wasm32-wasip2",
                output_path,
                validation,
            })
        }
        ServiceKind::Frontend => {
            let output_path = build_frontend_service(service).await?;
            let validation = validate_frontend_output_dir(service)?;
            Ok(BuildOutcome {
                mode: frontend_build_mode(service.manifest()),
                output_path,
                validation,
            })
        }
        ServiceKind::Agent => {
            let validation = validate_agent_service(service)?;
            Ok(BuildOutcome {
                mode: "agent-container-config",
                output_path: service.service_dir().to_path_buf(),
                validation,
            })
        }
    }
}

pub async fn build_metadata(service: &ServiceContext<'_>) -> Result<BTreeMap<String, String>> {
    let mut metadata = BTreeMap::new();
    let published_plan = service
        .project()
        .compiled_plan()
        .published_service_plan(service.name())?;
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
            ServiceKind::Frontend => frontend_build_mode(service.manifest()).to_owned(),
            ServiceKind::Agent => "agent-container-config".to_owned(),
        },
    );
    metadata.insert(
        PUBLISHED_SERVICE_PLAN_BUILD_METADATA_KEY.to_owned(),
        serde_json::to_string(&published_plan).context("serializing published service plan")?,
    );
    Ok(metadata)
}

pub async fn prepare_publish_artifact(service: &ServiceContext<'_>) -> Result<PublishArtifact> {
    match service.manifest().kind {
        ServiceKind::Http => {
            let loaded = service.http_service_manifest()?;
            let validation = validate_http_artifact(&loaded)?;
            let component_path = loaded.component_path();
            let component_signature = load_component_signature(&component_path)?;
            Ok(PublishArtifact {
                metadata: build_metadata(service).await?,
                validation,
                kind: PublishArtifactKind::Http {
                    component_path,
                    component_signature,
                },
            })
        }
        ServiceKind::Frontend => {
            let validation = validate_frontend_output_dir(service)?;
            let bundle_path = create_frontend_bundle(service, &validation.artifact_path).await?;
            Ok(PublishArtifact {
                metadata: build_metadata(service).await?,
                validation,
                kind: PublishArtifactKind::Frontend { bundle_path },
            })
        }
        ServiceKind::Agent => {
            let validation = validate_agent_service(service)?;
            let bundle_path = create_agent_bundle(service).await?;
            Ok(PublishArtifact {
                metadata: build_metadata(service).await?,
                validation,
                kind: PublishArtifactKind::Agent { bundle_path },
            })
        }
    }
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
    if is_internal_frontend_copy_command(program, args) {
        let (source_dir, output_dir) = parse_internal_frontend_copy_args(args)?;
        copy_frontend_static_site(&service_dir, &source_dir, &output_dir)?;
    } else {
        run_command(&service_dir, program, args.iter().map(String::as_str)).await?;
    }
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
    let file = File::create(&bundle_path)
        .with_context(|| format!("creating {}", bundle_path.display()))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut archive = Builder::new(encoder);
    archive
        .append_dir_all(".", output_dir)
        .with_context(|| format!("archiving {}", output_dir.display()))?;
    let encoder = archive
        .into_inner()
        .context("finalizing tar archive writer failed")?;
    encoder.finish().context("finalizing gzip archive failed")?;
    Ok(bundle_path)
}

fn kind_name(kind: ServiceKind) -> &'static str {
    match kind {
        ServiceKind::Http => "http",
        ServiceKind::Frontend => "frontend",
        ServiceKind::Agent => "agent",
    }
}

async fn create_agent_bundle(service: &ServiceContext<'_>) -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let bundle_name = format!("ignis-agent-{}-{nanos}.tar.gz", service.name());
    let bundle_path = std::env::temp_dir().join(bundle_name);
    let codex_config_files = match service.manifest().agent_runtime {
        AgentRuntime::Codex => {
            let agent = effective_agent_service_config(
                service.manifest().agent_runtime,
                service.manifest().agent.as_ref(),
            );
            let bytes = serde_json::to_vec_pretty(&agent).context("serializing agent config")?;
            let config_files = discover_codex_config_files(&service.service_dir())?;
            Some((bytes, config_files))
        }
        AgentRuntime::Opencode => None,
    };
    let opencode_config = match service.manifest().agent_runtime {
        AgentRuntime::Codex => None,
        AgentRuntime::Opencode => {
            let config_path = service.service_dir().join("opencode.json");
            Some(fs::read(&config_path).with_context(|| {
                format!(
                    "reading OpenCode config for agent service at {}",
                    config_path.display()
                )
            })?)
        }
    };
    let skills_dir = service.service_dir().join("skills");
    validate_agent_skills_dir(&skills_dir)?;
    let agents_md_path = service.service_dir().join("AGENTS.md");
    validate_agents_md_file(&agents_md_path)?;

    let file = File::create(&bundle_path)
        .with_context(|| format!("creating {}", bundle_path.display()))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut archive = Builder::new(encoder);
    if let Some((agent_json, config_files)) = codex_config_files {
        append_bytes_to_archive(&mut archive, "agent.json", &agent_json)?;
        for (archive_name, path) in config_files {
            archive
                .append_path_with_name(&path, archive_name)
                .with_context(|| format!("archiving {}", path.display()))?;
        }
    }
    if let Some(bytes) = opencode_config {
        append_bytes_to_archive(&mut archive, "opencode.json", &bytes)?;
    }
    if agents_md_path.exists() {
        archive
            .append_path_with_name(&agents_md_path, "AGENTS.md")
            .with_context(|| format!("archiving {}", agents_md_path.display()))?;
    }
    if skills_dir.exists() {
        archive
            .append_dir_all("skills", &skills_dir)
            .with_context(|| format!("archiving {}", skills_dir.display()))?;
    }
    let encoder = archive
        .into_inner()
        .context("finalizing tar archive writer failed")?;
    encoder.finish().context("finalizing gzip archive failed")?;
    Ok(bundle_path)
}

fn discover_codex_config_files(service_dir: &Path) -> Result<Vec<(&'static str, PathBuf)>> {
    let auth_path = service_dir.join("auth.json");
    let config_path = service_dir.join("config.toml");
    let auth_exists = auth_path.exists();
    let config_exists = config_path.exists();
    if auth_exists != config_exists {
        bail!(
            "Codex agent service must provide both auth.json and config.toml when using file-based Codex auth"
        );
    }
    if !auth_exists {
        return Ok(Vec::new());
    }
    validate_agent_config_file(&auth_path, "Codex auth.json")?;
    validate_agent_config_file(&config_path, "Codex config.toml")?;
    Ok(vec![("auth.json", auth_path), ("config.toml", config_path)])
}

fn append_bytes_to_archive<W: io::Write>(
    archive: &mut Builder<W>,
    path: &str,
    bytes: &[u8],
) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    archive
        .append_data(&mut header, path, bytes)
        .with_context(|| format!("archiving {path}"))
}

fn validate_agent_skills_dir(skills_dir: &Path) -> Result<usize> {
    if !skills_dir.exists() {
        return Ok(0);
    }
    let metadata = fs::symlink_metadata(skills_dir)
        .with_context(|| format!("reading {}", skills_dir.display()))?;
    if metadata.file_type().is_symlink() {
        bail!(
            "agent skills directory cannot be a symlink: {}",
            skills_dir.display()
        );
    }
    if !metadata.is_dir() {
        bail!(
            "agent skills path must be a directory: {}",
            skills_dir.display()
        );
    }

    let mut count = 0usize;
    for entry in
        fs::read_dir(skills_dir).with_context(|| format!("reading {}", skills_dir.display()))?
    {
        let entry = entry.with_context(|| format!("reading {}", skills_dir.display()))?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).with_context(|| format!("reading {}", path.display()))?;
        if metadata.file_type().is_symlink() {
            bail!("agent skill entries cannot be symlinks: {}", path.display());
        }
        if !metadata.is_dir() {
            bail!(
                "agent skills directory entries must be skill directories: {}",
                path.display()
            );
        }
        let skill_file = path.join("SKILL.md");
        if !skill_file.is_file() {
            bail!(
                "agent skill directory {} must contain SKILL.md",
                path.display()
            );
        }
        validate_no_symlinks(&path)?;
        count += 1;
    }
    Ok(count)
}

fn validate_agents_md_file(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("reading {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!("agent AGENTS.md cannot be a symlink: {}", path.display());
    }
    if !metadata.is_file() {
        bail!("agent AGENTS.md path must be a file: {}", path.display());
    }
    Ok(())
}

fn validate_agent_config_file(path: &Path, label: &str) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("reading {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!("{label} cannot be a symlink: {}", path.display());
    }
    if !metadata.is_file() {
        bail!("{label} path must be a file: {}", path.display());
    }
    Ok(())
}

fn validate_no_symlinks(path: &Path) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("reading {}", path.display()))? {
        let entry = entry.with_context(|| format!("reading {}", path.display()))?;
        let child = entry.path();
        let metadata =
            fs::symlink_metadata(&child).with_context(|| format!("reading {}", child.display()))?;
        if metadata.file_type().is_symlink() {
            bail!("agent skill files cannot be symlinks: {}", child.display());
        }
        if metadata.is_dir() {
            validate_no_symlinks(&child)?;
        }
    }
    Ok(())
}

fn validate_agent_service(service: &ServiceContext<'_>) -> Result<ArtifactValidation> {
    let agent = effective_agent_service_config(
        service.manifest().agent_runtime,
        service.manifest().agent.as_ref(),
    );
    agent.validate(service.name(), service.manifest().agent_runtime)?;
    let mut checks = vec![
        ValidationCheck {
            name: "runtime",
            detail: format!(
                "agent runtime is {}",
                service.manifest().agent_runtime.as_str()
            ),
        },
        ValidationCheck {
            name: "image",
            detail: format!("agent image is {}", agent.image),
        },
        ValidationCheck {
            name: "port",
            detail: format!("agent container port is {}", agent.port),
        },
    ];
    match service.manifest().agent_runtime {
        AgentRuntime::Codex => {
            let codex_configs = discover_codex_config_files(&service.service_dir())?;
            if codex_configs.is_empty() {
                checks.push(ValidationCheck {
                    name: "codex_config",
                    detail:
                        "Codex auth.json/config.toml not bundled; runtime will use env-based auth"
                            .to_owned(),
                });
            } else {
                checks.push(ValidationCheck {
                    name: "codex_config",
                    detail: "Codex auth.json and config.toml will be bundled".to_owned(),
                });
            }
        }
        AgentRuntime::Opencode => {
            let config_path = service.service_dir().join("opencode.json");
            let bytes = fs::read(&config_path).with_context(|| {
                format!("OpenCode agent service requires {}", config_path.display())
            })?;
            serde_json::from_slice::<serde_json::Value>(&bytes)
                .with_context(|| format!("parsing {}", config_path.display()))?;
            checks.push(ValidationCheck {
                name: "opencode_config",
                detail: format!("OpenCode config is {}", config_path.display()),
            });
        }
    }
    let skill_count = validate_agent_skills_dir(&service.service_dir().join("skills"))?;
    checks.push(ValidationCheck {
        name: "agent_skills",
        detail: if skill_count == 0 {
            "no custom agent skills bundled".to_owned()
        } else {
            format!("bundles {skill_count} custom agent skill(s)")
        },
    });
    let agents_md_path = service.service_dir().join("AGENTS.md");
    validate_agents_md_file(&agents_md_path)?;
    checks.push(ValidationCheck {
        name: "agents_md",
        detail: if agents_md_path.exists() {
            format!(
                "bundles agent prompt extension {}",
                agents_md_path.display()
            )
        } else {
            "no AGENTS.md prompt extension bundled".to_owned()
        },
    });
    Ok(ArtifactValidation {
        kind: "agent-container-config",
        artifact_path: service.service_dir().to_path_buf(),
        checks,
    })
}

fn frontend_build_mode(service: &ignis_manifest::ServiceManifest) -> &'static str {
    let Some(frontend) = &service.frontend else {
        return "frontend-build-command";
    };
    let Some(program) = frontend.build_command.first() else {
        return "frontend-build-command";
    };
    if is_internal_frontend_copy_command(program, &frontend.build_command[1..]) {
        "ignis-internal-copy-static-site"
    } else {
        "frontend-build-command"
    }
}

fn validate_http_artifact(loaded: &LoadedManifest) -> Result<ArtifactValidation> {
    let component_path = loaded.component_path();
    let metadata = fs::metadata(&component_path)
        .with_context(|| format!("reading {}", component_path.display()))?;
    if !metadata.is_file() {
        bail!(
            "component artifact {} exists but is not a file",
            component_path.display()
        );
    }
    if metadata.len() == 0 {
        bail!("component artifact {} is empty", component_path.display());
    }
    if component_path.extension().and_then(|ext| ext.to_str()) != Some("wasm") {
        bail!(
            "component artifact {} must end with .wasm",
            component_path.display()
        );
    }
    Ok(ArtifactValidation {
        kind: "http-component",
        artifact_path: component_path,
        checks: vec![
            ValidationCheck {
                name: "exists",
                detail: "component artifact exists".to_owned(),
            },
            ValidationCheck {
                name: "file",
                detail: "component artifact is a regular file".to_owned(),
            },
            ValidationCheck {
                name: "non_empty",
                detail: format!("component artifact size is {} bytes", metadata.len()),
            },
            ValidationCheck {
                name: "extension",
                detail: "component artifact uses .wasm extension".to_owned(),
            },
        ],
    })
}

fn validate_frontend_output_dir(service: &ServiceContext<'_>) -> Result<ArtifactValidation> {
    let frontend = service.manifest().frontend.as_ref().ok_or_else(|| {
        anyhow!(
            "frontend service `{}` is missing frontend config",
            service.name()
        )
    })?;
    let output_dir = service.service_dir().join(&frontend.output_dir);
    let metadata =
        fs::metadata(&output_dir).with_context(|| format!("reading {}", output_dir.display()))?;
    if !metadata.is_dir() {
        bail!(
            "frontend output path {} exists but is not a directory",
            output_dir.display()
        );
    }

    let mut file_count = 0usize;
    count_files(&output_dir, &mut file_count)?;
    if file_count == 0 {
        bail!(
            "frontend output directory {} does not contain any files",
            output_dir.display()
        );
    }

    let index_path = output_dir.join("index.html");
    if !index_path.exists() {
        bail!(
            "frontend output directory {} is missing index.html",
            output_dir.display()
        );
    }

    Ok(ArtifactValidation {
        kind: "frontend-static-site",
        artifact_path: output_dir,
        checks: vec![
            ValidationCheck {
                name: "exists",
                detail: "frontend output directory exists".to_owned(),
            },
            ValidationCheck {
                name: "directory",
                detail: "frontend output path is a directory".to_owned(),
            },
            ValidationCheck {
                name: "non_empty",
                detail: format!("frontend output directory contains {file_count} files"),
            },
            ValidationCheck {
                name: "entrypoint",
                detail: "frontend output directory contains index.html".to_owned(),
            },
        ],
    })
}

async fn create_frontend_bundle(
    service: &ServiceContext<'_>,
    output_dir: &Path,
) -> Result<PathBuf> {
    create_tarball(output_dir, service.name()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("ignis-cli-{name}-{nanos}"))
    }

    #[test]
    fn validates_agent_skills_dir() {
        let root = temp_test_dir("skills-ok");
        let skill = root.join("skills").join("demo");
        fs::create_dir_all(skill.join("references")).unwrap();
        fs::write(skill.join("SKILL.md"), "# Demo\n").unwrap();
        fs::write(skill.join("references").join("notes.md"), "notes\n").unwrap();

        let count = validate_agent_skills_dir(&root.join("skills")).unwrap();
        assert_eq!(count, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_agent_skill_without_skill_md() {
        let root = temp_test_dir("skills-missing");
        fs::create_dir_all(root.join("skills").join("demo")).unwrap();

        let error = validate_agent_skills_dir(&root.join("skills")).unwrap_err();
        assert!(error.to_string().contains("must contain SKILL.md"));
        let _ = fs::remove_dir_all(root);
    }
}

fn is_internal_frontend_copy_command(program: &str, args: &[String]) -> bool {
    program == "ignis"
        && args.first().map(String::as_str) == Some("internal")
        && args.get(1).map(String::as_str) == Some("copy-frontend-static")
}

fn parse_internal_frontend_copy_args(args: &[String]) -> Result<(PathBuf, PathBuf)> {
    let mut source_dir = None;
    let mut output_dir = None;
    let mut index = 2usize;
    while index < args.len() {
        match args[index].as_str() {
            "--source-dir" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow!("missing value for --source-dir"))?;
                source_dir = Some(PathBuf::from(value));
            }
            "--output-dir" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow!("missing value for --output-dir"))?;
                output_dir = Some(PathBuf::from(value));
            }
            other => bail!("unexpected ignis internal frontend build argument `{other}`"),
        }
        index += 1;
    }

    Ok((
        source_dir.unwrap_or_else(|| PathBuf::from("src")),
        output_dir.unwrap_or_else(|| PathBuf::from("dist")),
    ))
}

fn copy_frontend_static_site(
    service_dir: &Path,
    source_dir: &Path,
    output_dir: &Path,
) -> Result<()> {
    let source_path = service_dir.join(source_dir);
    let output_path = service_dir.join(output_dir);
    if !source_path.exists() {
        bail!(
            "frontend source directory {} does not exist",
            source_path.display()
        );
    }
    if !source_path.is_dir() {
        bail!(
            "frontend source path {} is not a directory",
            source_path.display()
        );
    }
    if output_path.exists() {
        fs::remove_dir_all(&output_path)
            .with_context(|| format!("removing {}", output_path.display()))?;
    }
    fs::create_dir_all(&output_path)
        .with_context(|| format!("creating {}", output_path.display()))?;
    copy_dir_contents(&source_path, &output_path)
}

fn copy_dir_contents(source_dir: &Path, output_dir: &Path) -> Result<()> {
    for entry in
        fs::read_dir(source_dir).with_context(|| format!("reading {}", source_dir.display()))?
    {
        let entry = entry.with_context(|| format!("reading {}", source_dir.display()))?;
        let source_path = entry.path();
        let destination_path = output_dir.join(entry.file_name());
        let file_type = entry
            .file_type()
            .with_context(|| format!("reading {}", source_path.display()))?;
        if file_type.is_dir() {
            fs::create_dir_all(&destination_path)
                .with_context(|| format!("creating {}", destination_path.display()))?;
            copy_dir_contents(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).with_context(|| {
                format!(
                    "copying {} -> {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        } else if file_type.is_symlink() {
            bail!(
                "frontend source path {} contains a symlink; static frontend build only supports regular files and directories",
                source_path.display()
            );
        }
    }
    Ok(())
}

fn count_files(path: &Path, count: &mut usize) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            count_files(&entry.path(), count)?;
        } else if file_type.is_file() {
            *count += 1;
        }
    }
    Ok(())
}
