use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub const DEFAULT_REGION: Region = Region::Global;
pub const GLOBAL_SERVER: &str = "https://igniscloud.dev/api";
pub const CN_SERVER: &str = "https://api.transairobot.com/api";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Global,
    Cn,
}

impl Region {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Cn => "cn",
        }
    }

    pub fn server(self) -> &'static str {
        match self {
            Self::Global => GLOBAL_SERVER,
            Self::Cn => CN_SERVER,
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "global" => Ok(Self::Global),
            "cn" | "china" => Ok(Self::Cn),
            other => bail!("unknown Ignis region `{other}`; expected `cn` or `global`"),
        }
    }

    pub fn infer_from_server(server: &str) -> Self {
        if server.contains("transairobot.com") {
            Self::Cn
        } else {
            Self::Global
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    #[serde(default = "default_region")]
    pub region: Region,
    #[serde(default = "default_server")]
    pub server: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub user_sub: Option<String>,
    #[serde(default)]
    pub user_aud: Option<String>,
    #[serde(default)]
    pub user_display_name: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub accounts: BTreeMap<String, RegionAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionAccount {
    #[serde(default)]
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
        Self::resolve_for_region(token_override, None)
    }

    pub fn resolve_for_region(
        token_override: Option<String>,
        region_override: Option<Region>,
    ) -> Result<Self> {
        let mut config = Self::load()?.unwrap_or_else(|| Self {
            region: DEFAULT_REGION,
            server: default_server_for_region(DEFAULT_REGION),
            token: String::new(),
            user_sub: None,
            user_aud: None,
            user_display_name: None,
            accounts: BTreeMap::new(),
        });
        let region = region_override.unwrap_or(config.region);
        config.region = region;

        if let Some(token) = token_override
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .or_else(load_token_from_env)
        {
            config.token = token;
            config.server =
                load_server_from_env().unwrap_or_else(|| default_server_for_region(config.region));
            return config.validate_resolved();
        }
        if let Some(server) = load_server_from_env() {
            config.server = server;
            config.region = Region::infer_from_server(&config.server);
        } else {
            let account = config.account(config.region).cloned();
            config.server = account
                .as_ref()
                .and_then(|account| {
                    let server = account.server.trim();
                    (!server.is_empty()).then(|| server.to_owned())
                })
                .unwrap_or_else(|| default_server_for_region(config.region));
            if let Some(account) = account.as_ref() {
                config.token = account.token.clone();
                config.user_sub = account.user_sub.clone();
                config.user_aud = account.user_aud.clone();
                config.user_display_name = account.user_display_name.clone();
            }
        }
        config.validate_resolved()
    }

    fn validate_resolved(self) -> Result<Self> {
        if self.token.trim().is_empty() {
            bail!(
                "missing API token or login session for region `{}`; run `ignis login --region {}`, pass `--token`, or set IGNIS_TOKEN",
                self.region.as_str(),
                self.region.as_str()
            );
        }
        Ok(self)
    }

    pub fn load() -> Result<Option<Self>> {
        let Some(path) = existing_config_path() else {
            return Ok(None);
        };
        let raw =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut config: CliConfig =
            toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        config.migrate_legacy_account();
        if config.server.trim().is_empty() || !raw.contains("server") {
            config.server = default_server_for_region(config.region);
        } else if !raw.contains("region") {
            config.region = Region::infer_from_server(&config.server);
        }
        Ok(Some(config))
    }

    pub fn set_account(
        &mut self,
        region: Region,
        token: String,
        user_sub: Option<String>,
        user_aud: Option<String>,
        user_display_name: Option<String>,
    ) {
        self.region = region;
        self.server = region.server().to_owned();
        self.token = token.clone();
        self.user_sub = user_sub.clone();
        self.user_aud = user_aud.clone();
        self.user_display_name = user_display_name.clone();
        self.accounts.insert(
            region.as_str().to_owned(),
            RegionAccount {
                server: region.server().to_owned(),
                token,
                user_sub,
                user_aud,
                user_display_name,
            },
        );
    }

    fn account(&self, region: Region) -> Option<&RegionAccount> {
        self.accounts.get(region.as_str())
    }

    fn migrate_legacy_account(&mut self) {
        let token = self.token.trim();
        if token.is_empty() || self.accounts.contains_key(self.region.as_str()) {
            return;
        }
        self.accounts.insert(
            self.region.as_str().to_owned(),
            RegionAccount {
                server: self.server.clone(),
                token: token.to_owned(),
                user_sub: self.user_sub.clone(),
                user_aud: self.user_aud.clone(),
                user_display_name: self.user_display_name.clone(),
            },
        );
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

fn default_region() -> Region {
    DEFAULT_REGION
}

fn default_server() -> String {
    default_server_for_region(DEFAULT_REGION)
}

fn default_server_for_region(region: Region) -> String {
    region.server().to_owned()
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

fn load_server_from_env() -> Option<String> {
    for key in ["IGNIS_SERVER", "IGNISCLOUD_API_URL", "IGNISCLOUD_SERVER"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}
