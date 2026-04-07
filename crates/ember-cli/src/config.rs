use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub const DEFAULT_SERVER: &str = "https://embercloud.transairobot.com/api";

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
        if let Some(token) = token_override
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .or_else(load_token_from_env)
        {
            return Ok(Self::from_token(token));
        }

        let path = load_path_candidates()
            .into_iter()
            .find(|path| path.exists())
            .unwrap_or_else(config_path);
        if !path.exists() {
            bail!(
                "missing API token or app token; pass `--token`, set EMBER_TOKEN, or keep a legacy CLI config at {}",
                config_path().display()
            );
        }

        let raw =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut config: CliConfig =
            toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        if config.server.trim().is_empty() {
            config.server = default_server();
        }
        if config.token.trim().is_empty() {
            bail!("invalid CLI config at {}", path.display());
        }
        Ok(config)
    }

    pub fn from_token(token: String) -> Self {
        Self {
            server: default_server(),
            token,
            user_sub: None,
            user_aud: None,
            user_display_name: None,
        }
    }
}

fn default_server() -> String {
    DEFAULT_SERVER.to_owned()
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ember")
        .join("config.toml")
}

fn load_path_candidates() -> Vec<PathBuf> {
    let Some(base) = dirs::config_dir() else {
        return vec![config_path()];
    };

    vec![
        base.join("ember").join("config.toml"),
        base.join("embercloud").join("config.toml"),
        base.join("wkr").join("config.toml"),
    ]
}

fn load_token_from_env() -> Option<String> {
    for key in ["EMBER_TOKEN", "EMBERCLOUD_TOKEN", "WKR_API_TOKEN"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}
