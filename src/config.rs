use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, RwLockReadGuard};

/// As config may be updated by reloads,
/// it is proxied behind this provider.
/// TODO: reload
pub struct ConfigProvider {
    config: RwLock<RootConfig>,
}

/// Configuration file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootConfig {
    #[serde(skip)]
    pub config_file: PathBuf,

    /// Address to listen on.
    #[serde(alias = "bind")]
    pub bind_address: String,
    /// Address to create proxying UDP sockets on.
    pub proxy_bind: String,

    /// Load balancing method. Defaults to [`LoadBalanceMethod::RoundRobin`].
    pub load_balance_method: Option<LoadBalanceMethod>,
    /// Whether proxy protocol should be used. Defaults to true.
    pub proxy_protocol: Option<bool>,
    /// Backend to route players to.
    pub backend: BackendConfig,
}

/// Load balancing method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadBalanceMethod {
    RoundRobin,
    LeastConnected,
}

/// Configuration for a backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    /// Rate, in seconds, at which to fetch MOTD information.
    pub motd_refresh_rate: u64,
    /// Address of the server to ping to get MOTD information.
    pub motd_source: Option<String>,
    /// Servers to proxy players to.
    pub servers: Vec<BackendServerConfig>,
}

/// Configuration for a backend server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendServerConfig {
    /// Address of the server.
    pub address: String,
}

/// Attempts to read the configuration file.
///
/// ## Arguments
///
/// * `config_file` - Config file path
pub fn read_config(config_file: PathBuf) -> anyhow::Result<RootConfig> {
    let contents = fs::read_to_string(&config_file)?;
    let mut config: RootConfig = toml::from_str(&contents)?;
    config.config_file = config_file;
    Ok(config)
}

impl ConfigProvider {
    pub fn new(config: RootConfig) -> Self {
        Self {
            config: RwLock::new(config),
        }
    }

    #[inline]
    pub async fn read(&self) -> RwLockReadGuard<'_, RootConfig> {
        self.config.read().await
    }
}
