use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use ignis_manifest::{
    CompiledProjectPlan, LoadedManifest, LoadedProjectManifest, PROJECT_MANIFEST_FILE,
    ProjectManifest, ServiceManifest,
};

use crate::project_state::ProjectState;

#[derive(Debug, Clone)]
pub struct ProjectContext {
    loaded: LoadedProjectManifest,
    state: Option<ProjectState>,
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
        let state = ProjectState::load_optional(&loaded.project_dir)?;
        Ok(Self { loaded, state })
    }

    pub fn load_optional() -> Result<Option<Self>> {
        match Self::load() {
            Ok(context) => Ok(Some(context)),
            Err(error)
                if error
                    .to_string()
                    .contains(&format!("could not find `{PROJECT_MANIFEST_FILE}`")) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    pub fn manifest(&self) -> &ProjectManifest {
        &self.loaded.manifest
    }

    pub fn compiled_plan(&self) -> &CompiledProjectPlan {
        &self.loaded.compiled_plan
    }

    pub fn project_name(&self) -> &str {
        self.loaded.project_name()
    }

    pub fn project_id(&self) -> Option<&str> {
        self.state.as_ref().and_then(ProjectState::project_id)
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

    pub fn project_domain(&self) -> Option<&str> {
        self.manifest()
            .project
            .domain
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn matches_project_ref(&self, project_ref: &str) -> bool {
        let project_ref = project_ref.trim();
        !project_ref.is_empty()
            && (self.project_name() == project_ref || self.project_id() == Some(project_ref))
    }

    pub fn set_project_domain(&self, domain: &str) -> Result<()> {
        let updated = update_project_domain_in_text(
            &fs::read_to_string(self.manifest_path())
                .with_context(|| format!("reading {}", self.manifest_path().display()))?,
            domain,
        )?;
        fs::write(self.manifest_path(), updated)
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

fn update_project_domain_in_text(raw: &str, domain: &str) -> Result<String> {
    let domain = domain.trim().trim_matches('.').to_ascii_lowercase();
    if domain.is_empty() {
        bail!("project domain cannot be empty");
    }

    let mut lines = raw
        .split_inclusive('\n')
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        bail!("ignis.hcl is empty");
    }

    let Some(project_start) = lines.iter().position(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("project") && trimmed.contains('=') && trimmed.contains('{')
    }) else {
        bail!("ignis.hcl is missing `project = {{ ... }}`");
    };

    let mut depth = 0i32;
    let mut project_end = None;
    for (index, line) in lines.iter().enumerate().skip(project_start) {
        for ch in line.chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        project_end = Some(index);
                        break;
                    }
                }
                _ => {}
            }
        }
        if project_end.is_some() {
            break;
        }
    }
    let Some(project_end) = project_end else {
        bail!("ignis.hcl project block is missing closing `}}`");
    };

    let name_or_domain_line = lines[project_start + 1..project_end]
        .iter()
        .find(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("name")
                || trimmed.starts_with("\"name\"")
                || trimmed.starts_with("domain")
                || trimmed.starts_with("\"domain\"")
        });
    let indent = name_or_domain_line
        .map(|line| {
            line.chars()
                .take_while(|ch| ch.is_ascii_whitespace())
                .collect::<String>()
        })
        .unwrap_or_else(|| "  ".to_owned());
    let quoted_keys = name_or_domain_line
        .map(|line| line.trim_start().starts_with('"'))
        .unwrap_or(false);

    let domain_line = if quoted_keys {
        format!("{indent}\"domain\" = \"{domain}\"")
    } else {
        format!("{indent}domain = \"{domain}\"")
    };

    let domain_index = (project_start + 1..project_end).find(|index| {
        let trimmed = lines[*index].trim_start();
        trimmed.starts_with("domain") || trimmed.starts_with("\"domain\"")
    });

    if let Some(index) = domain_index {
        let newline = if lines[index].ends_with('\n') { "\n" } else { "" };
        lines[index] = format!("{domain_line}{newline}");
    } else {
        let insert_at = (project_start + 1..project_end)
            .find(|index| {
                let trimmed = lines[*index].trim_start();
                trimmed.starts_with("name") || trimmed.starts_with("\"name\"")
            })
            .map(|index| index + 1)
            .unwrap_or(project_end);
        lines.insert(insert_at, format!("{domain_line}\n"));
    }

    Ok(lines.concat())
}

#[cfg(test)]
mod tests {
    use super::update_project_domain_in_text;

    #[test]
    fn inserts_project_domain_without_rewriting_other_fields() {
        let input = "project = {\n  name = \"demo\"\n}\n\nlisteners = []\n";
        let output = update_project_domain_in_text(input, "foo.transairobot.com").unwrap();
        assert_eq!(
            output,
            "project = {\n  name = \"demo\"\n  domain = \"foo.transairobot.com\"\n}\n\nlisteners = []\n"
        );
    }

    #[test]
    fn updates_existing_project_domain_in_place() {
        let input = "project = {\n  name = \"demo\"\n  domain = \"old.example.com\"\n}\n";
        let output = update_project_domain_in_text(input, "new.example.com").unwrap();
        assert_eq!(
            output,
            "project = {\n  name = \"demo\"\n  domain = \"new.example.com\"\n}\n"
        );
    }
}
