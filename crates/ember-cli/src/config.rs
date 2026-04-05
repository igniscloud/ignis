use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
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
    pub fn load() -> Result<Self> {
        let path = load_path()?;
        let raw =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let config: CliConfig =
            toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        if config.server.trim().is_empty() || config.token.trim().is_empty() {
            bail!("invalid CLI config at {}", path.display());
        }
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let raw = toml::to_string_pretty(self).context("serializing CLI config")?;
        fs::write(&path, raw).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn delete() -> Result<()> {
        let path = config_path()?;
        if path.exists() {
            fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
        Ok(())
    }
}

fn config_path() -> Result<PathBuf> {
    let base = dirs::config_dir().context("discovering config directory")?;
    Ok(base.join("ember").join("config.toml"))
}

fn load_path() -> Result<PathBuf> {
    let new_path = config_path()?;
    if new_path.exists() {
        return Ok(new_path);
    }

    let base = dirs::config_dir().context("discovering config directory")?;
    for legacy_path in [
        base.join("flickercloud").join("config.toml"),
        base.join("wkr").join("config.toml"),
    ] {
        if legacy_path.exists() {
            return Ok(legacy_path);
        }
    }

    Ok(new_path)
}
