use std::path::PathBuf;

use log::log_enabled;
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, RwLock, RwLockReadGuard};

/// As config may be updated by reloads,
/// it is proxied behind this provider.
pub struct ConfigProvider {
    /// Config file path. Used for reloads.
    config_file: PathBuf,

    /// Last parsed config.
    config: RwLock<RootConfig>,
    /// Reload notifier.
    reload_notify: Notify,
}

/// Configuration file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootConfig {
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
    /// Rate, in seconds, at which to ping servers to check health.
    pub health_check_rate: u64,
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

impl ConfigProvider {
    pub fn new(config_file: PathBuf, config: RootConfig) -> Self {
        Self {
            config_file,
            config: RwLock::new(config),
            reload_notify: Notify::new(),
        }
    }
}

/// Attempts to read the configuration file.
///
/// ## Arguments
///
/// * `config_file` - Config file path
///
/// ## Returns
///
/// A [`ConfigProvider`] that is guaranteed to have the config already loaded and without errors.
pub fn read_config(config_file: PathBuf) -> anyhow::Result<ConfigProvider> {
    let contents = std::fs::read_to_string(&config_file)?;
    let config: RootConfig = toml::from_str(&contents)?;
    let config_provider = ConfigProvider {
        config_file,
        config: RwLock::new(config),
        reload_notify: Notify::new(),
    };
    Ok(config_provider)
}

impl ConfigProvider {
    #[inline]
    pub async fn read(&self) -> RwLockReadGuard<'_, RootConfig> {
        self.config.read().await
    }

    #[inline]
    pub async fn wait_reload(&self) {
        self.reload_notify.notified().await;
    }

    /// Reloads the configuration.
    pub async fn reload(&self) {
        let config = match self.read_config().await {
            Ok(config) => config,
            Err(err) => {
                log::error!("Unable to reload config file: {:?}", err);
                return;
            }
        };
        let mut w = self.config.write().await;
        *w = config;
        drop(w);
        log::info!("Config file reloaded.");
        if log_enabled!(log::Level::Debug) {
            let config = self.read().await;
            log::debug!("Parsed configuration: {:#?}", config);
        }
        self.reload_notify.notify_waiters();
    }

    async fn read_config(&self) -> anyhow::Result<RootConfig> {
        let contents = tokio::fs::read_to_string(&self.config_file).await?;
        let config: RootConfig = toml::from_str(&contents)?;
        Ok(config)
    }
}
