use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use ignis_manifest::{
    LoadedManifest, LoadedProjectManifest, PROJECT_MANIFEST_FILE, ProjectManifest, ServiceManifest,
};

#[derive(Debug, Clone)]
pub struct ProjectContext {
    loaded: LoadedProjectManifest,
}

#[derive(Debug, Clone, Copy)]
pub struct ServiceContext<'a> {
    project: &'a ProjectContext,
    manifest: &'a ServiceManifest,
}

impl ProjectContext {
    pub fn load() -> Result<Self> {
        let manifest_path = find_project_manifest_path(std::env::current_dir()?)?;
        let loaded = LoadedProjectManifest::load(&manifest_path)?;
        Ok(Self { loaded })
    }

    pub fn manifest(&self) -> &ProjectManifest {
        &self.loaded.manifest
    }

    pub fn project_name(&self) -> &str {
        self.loaded.project_name()
    }

    pub fn manifest_path(&self) -> &Path {
        &self.loaded.manifest_path
    }

    pub fn project_dir(&self) -> &Path {
        &self.loaded.project_dir
    }

    pub fn find_service(&self, service_name: &str) -> Option<&ServiceManifest> {
        self.loaded.find_service(service_name)
    }

    pub fn service(&self, service_name: &str) -> Result<ServiceContext<'_>> {
        let manifest = self.find_service(service_name).ok_or_else(|| {
            anyhow!(
                "service `{service_name}` not found in {}",
                self.manifest_path().display()
            )
        })?;
        Ok(ServiceContext {
            project: self,
            manifest,
        })
    }

    pub fn save_manifest(&self, manifest: &ProjectManifest) -> Result<()> {
        fs::write(self.manifest_path(), manifest.render()?)
            .with_context(|| format!("writing {}", self.manifest_path().display()))
    }

    pub fn ensure_new_service_path_available(&self, service: &ServiceManifest) -> Result<()> {
        let new_path = normalized_relative_path(&service.path);
        for existing in &self.manifest().services {
            if normalized_relative_path(&existing.path) == new_path {
                bail!(
                    "service path `{}` is already used by service `{}`",
                    service.path.display(),
                    existing.name
                );
            }
        }

        let service_dir = self.project_dir().join(&service.path);
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
}

impl<'a> ServiceContext<'a> {
    pub fn project(&self) -> &'a ProjectContext {
        self.project
    }

    pub fn project_name(&self) -> &'a str {
        self.project.project_name()
    }

    pub fn manifest(&self) -> &'a ServiceManifest {
        self.manifest
    }

    pub fn name(&self) -> &'a str {
        &self.manifest.name
    }

    pub fn service_dir(&self) -> PathBuf {
        self.project.loaded.service_dir(self.manifest)
    }

    pub fn http_service_manifest(&self) -> Result<LoadedManifest> {
        self.project.loaded.http_service_manifest(self.name())
    }
}

pub fn normalized_relative_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
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
