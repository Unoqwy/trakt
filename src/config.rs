use std::path::Path;

use anyhow::Context;
use log::log_enabled;
use serde::{Deserialize, Serialize};
use trakt_core::{
    bedrock::RaknetProxyServer, config::BackendConfig, BackendLoadResult, ProxyServer,
};

/// Configuration file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootConfig {
    /// Address to listen on.
    #[serde(alias = "bind")]
    pub bind_address: String,
    /// Address to create proxying UDP sockets on.
    pub proxy_bind: String,

    /// Rate, in seconds, at which to ping servers to check health.
    pub health_check_rate: u64,
    /// Rate, in seconds, at which to fetch MOTD information.
    pub motd_refresh_rate: u64,

    /// Backend to route players to.
    pub backend: BackendConfig,
}

/// Reads the configuration file.
///
/// ## Arguments
///
/// * `config_file` - Config file path
pub async fn read_config<P: AsRef<Path>>(config_file: P) -> anyhow::Result<RootConfig> {
    let contents = tokio::fs::read_to_string(config_file).await?;
    let config: RootConfig = toml::from_str(&contents)?;
    Ok(config)
}

/// Reloads a bedrock proxy server.
///
/// ## Arguments
///
/// * `proxy_server` - Raknet proxy server
pub async fn reload_bedrock_proxy<P: AsRef<Path>>(
    proxy_server: &RaknetProxyServer,
    config_file: P,
) -> bool {
    let reload = || async move {
        let config = read_config(config_file).await?;
        if log_enabled!(log::Level::Debug) {
            log::debug!("Parsed configuration: {:#?}", config);
        }
        let backends = proxy_server.get_backends().await;
        let backend = backends.get(0).context("no backend")?;
        let result = backend.reload_config(&config.backend).await;
        anyhow::Result::<BackendLoadResult>::Ok(result)
    };
    match reload().await {
        Ok(result) => {
            log::info!(
                "Reloaded backend. There are now {} servers ({} added, {} removed)",
                result.server_count,
                result.new_count,
                result.removed_count
            );
            true
        }
        Err(err) => {
            log::error!("Couldn't reload configuration: {:?}", err);
            false
        }
    }
}
