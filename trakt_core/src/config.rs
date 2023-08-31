use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, RwLock, RwLockReadGuard};

/// As [`RuntimeConfig`] may be updated by reloads,
/// it is proxied behind this provider.
pub struct RuntimeConfigProvider {
    /// Last config.
    config: RwLock<RuntimeConfig>,
    /// Reload notifier.
    reload_notify: Notify,
}

/// Configuration for things that can be changed
/// at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Address to bind Proxy <-> Server connections to.
    pub proxy_bind: String,
    /// Rate, in seconds, at which to ping servers to check health.
    #[serde(default)]
    pub health_check_rate: u64,
    /// Rate, in seconds, at which to fetch MOTD information.
    #[serde(default)]
    pub motd_refresh_rate: u64,
}

/// Load balancing method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadBalanceMethod {
    /// Pick each server in turn.
    RoundRobin,
    /// Pick the least connected server.
    LeastConnected,
}

/// Configuration for a backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    /// Backend ID.
    #[serde(default = "default_backend_id")]
    pub id: String,
    /// Load balancing method.
    pub load_balance_method: LoadBalanceMethod,
    /// Whether proxy protocol should be used.
    pub proxy_protocol: bool,
    /// Server to ping to get MOTD information from.
    pub motd_source: Option<BackendServerConfig>,
    /// Servers to proxy players to.
    pub servers: Vec<BackendServerConfig>,
}

/// Configuration for a backend server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendServerConfig {
    /// Address of the server.
    pub address: String,
    /// Proxy protocol override. If set, the server will respect that setting over the global one.
    pub proxy_protocol: Option<bool>,
}

impl RuntimeConfigProvider {
    pub fn new(initial_config: RuntimeConfig) -> Self {
        Self {
            config: RwLock::new(initial_config),
            reload_notify: Notify::new(),
        }
    }

    #[inline]
    pub async fn read(&self) -> RwLockReadGuard<'_, RuntimeConfig> {
        self.config.read().await
    }

    #[inline]
    pub async fn wait_reload(&self) {
        self.reload_notify.notified().await;
    }

    /// Reloads the configuration.
    ///
    /// ## Arguments
    ///
    /// * `config` - New runtime config
    pub async fn reload(&self, config: RuntimeConfig) {
        let mut w = self.config.write().await;
        *w = config;
        drop(w);
        self.reload_notify.notify_waiters();
    }
}

fn default_backend_id() -> String {
    "default".to_owned()
}
