use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Region;

const PROJECT_STATE_DIR: &str = ".ignis";
const PROJECT_STATE_FILE: &str = "project.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectState {
    pub project_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,
}

impl ProjectState {
    pub fn new(project_name: impl Into<String>, project_id: Option<String>) -> Self {
        Self {
            project_name: project_name.into(),
            project_id: project_id.and_then(|value| normalized_optional_string(&value)),
            region: None,
        }
    }

    pub fn with_region(mut self, region: Region) -> Self {
        self.region = Some(region);
        self
    }

    pub fn load_optional(project_dir: &Path) -> Result<Option<Self>> {
        let state_path = project_state_path(project_dir);
        if !state_path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(&state_path)
            .with_context(|| format!("reading {}", state_path.display()))?;
        let state: Self = serde_json::from_str(&raw)
            .with_context(|| format!("parsing {}", state_path.display()))?;
        Ok(Some(state))
    }

    pub fn save(&self, project_dir: &Path) -> Result<PathBuf> {
        let state_dir = project_dir.join(PROJECT_STATE_DIR);
        fs::create_dir_all(&state_dir)
            .with_context(|| format!("creating {}", state_dir.display()))?;
        let state_path = state_dir.join(PROJECT_STATE_FILE);
        let rendered = serde_json::to_string_pretty(self).context("rendering project state")?;
        fs::write(&state_path, format!("{rendered}\n"))
            .with_context(|| format!("writing {}", state_path.display()))?;
        Ok(state_path)
    }

    pub fn project_id(&self) -> Option<&str> {
        self.project_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn region(&self) -> Option<Region> {
        self.region
    }
}

pub fn project_state_path(project_dir: &Path) -> PathBuf {
    project_dir.join(PROJECT_STATE_DIR).join(PROJECT_STATE_FILE)
}

pub fn project_state_from_response(response: &Value, fallback_project_name: &str) -> ProjectState {
    let payload = response.get("data").unwrap_or(response);
    let project_name = json_string(payload, &["name", "project_name"])
        .unwrap_or_else(|| fallback_project_name.to_owned());
    let project_id = json_string(payload, &["project_id", "id"]);
    ProjectState::new(project_name, project_id)
}

fn json_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(key)
            .and_then(Value::as_str)
            .and_then(normalized_optional_string)
    })
}

fn normalized_optional_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::project_state_from_response;

    #[test]
    fn parses_project_id_from_current_api_shape() {
        let state = project_state_from_response(
            &json!({
                "data": {
                    "name": "hello-project",
                    "project_id": "project-1234abcd"
                }
            }),
            "fallback-name",
        );

        assert_eq!(state.project_name, "hello-project");
        assert_eq!(state.project_id(), Some("project-1234abcd"));
    }

    #[test]
    fn falls_back_to_id_and_requested_name_when_needed() {
        let state = project_state_from_response(
            &json!({
                "data": {
                    "id": "project-1234abcd"
                }
            }),
            "fallback-name",
        );

        assert_eq!(state.project_name, "fallback-name");
        assert_eq!(state.project_id(), Some("project-1234abcd"));
    }
}
