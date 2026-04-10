use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub const DEFAULT_SERVER: &str = "https://igniscloud.dev/api";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    #[serde(default = "default_server")]
    pub server: String,
    pub token: String,
    #[serde(default)]
    pub user_sub: Option<String>,
    #[serde(default)]
    pub user_aud: Option<String>,
    #[serde(default)]
    pub user_display_name: Option<String>,
}

impl CliConfig {
    pub fn resolve(token_override: Option<String>) -> Result<Self> {
        let mut config = Self::load()?.unwrap_or_else(|| Self {
            server: default_server(),
            token: String::new(),
            user_sub: None,
            user_aud: None,
            user_display_name: None,
        });

        if let Some(token) = token_override
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .or_else(load_token_from_env)
        {
            config.token = token;
        }
        config.server = default_server();
        if config.token.trim().is_empty() {
            bail!(
                "missing API token or login session; run `ignis login`, pass `--token`, or set IGNIS_TOKEN"
            );
        }
        Ok(config)
    }

    pub fn load() -> Result<Option<Self>> {
        let Some(path) = existing_config_path() else {
            return Ok(None);
        };
        let raw =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut config: CliConfig =
            toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        config.server = default_server();
        Ok(Some(config))
    }

    pub fn save(&self) -> Result<PathBuf> {
        let path = config_path();
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("config path has no parent: {}", path.display()))?;
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        fs::write(&path, toml::to_string_pretty(self)?)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(path)
    }

    pub fn clear() -> Result<Option<PathBuf>> {
        let Some(path) = existing_config_path() else {
            return Ok(None);
        };
        fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        Ok(Some(path))
    }
}

fn default_server() -> String {
    DEFAULT_SERVER.to_owned()
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ignis")
        .join("config.toml")
}

pub fn default_config_path() -> PathBuf {
    config_path()
}

fn load_path_candidates() -> Vec<PathBuf> {
    let Some(base) = dirs::config_dir() else {
        return vec![config_path()];
    };

    vec![base.join("ignis").join("config.toml")]
}

fn existing_config_path() -> Option<PathBuf> {
    load_path_candidates()
        .into_iter()
        .find(|path| path.exists())
}

fn load_token_from_env() -> Option<String> {
    for key in ["IGNIS_TOKEN", "IGNISCLOUD_TOKEN"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}
