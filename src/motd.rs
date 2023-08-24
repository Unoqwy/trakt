use std::{sync::Arc, time::Duration};

use tokio::sync::{RwLock, Semaphore};

use crate::{
    config::ConfigProvider,
    raknet::ping::{self, MOTD},
};

/// A controller that periodically fetches MOTD information
/// from the backend and exposes the last successful response.
pub struct MOTDReflector {
    execute_lock: Semaphore,

    /// Config provider.
    config_provider: Arc<ConfigProvider>,

    /// Last successful MOTD response, if any.
    last_motd: RwLock<Option<MOTD>>,
}

impl MOTDReflector {
    pub fn new(config_provider: Arc<ConfigProvider>) -> Self {
        Self {
            execute_lock: Semaphore::new(1),
            config_provider,
            last_motd: RwLock::new(None),
        }
    }

    /// Returns a clone of the last sucessful MOTD information received.
    pub async fn last_motd(&self) -> Option<MOTD> {
        self.last_motd.read().await.clone()
    }

    /// Fetches the MOTD.
    pub async fn execute(&self) {
        let _permit = self.execute_lock.acquire().await;
        let (local_addr, sources, proxy_protocol) = {
            let config = self.config_provider.read().await;
            let sources = if let Some(source) = &config.backend.motd_source {
                vec![source.clone()]
            } else {
                config
                    .backend
                    .servers
                    .iter()
                    .map(|server| server.address.clone())
                    .collect()
            };
            let proxy_protocol = config.proxy_protocol.unwrap_or(true);
            (config.proxy_bind.clone(), sources, proxy_protocol)
        };
        log::debug!(
            "Fetching MOTD information from backend ({} sources)...",
            sources.len()
        );
        let timeout = Duration::from_secs(5);
        for source in sources.into_iter() {
            match ping::ping(&local_addr, &source, proxy_protocol, timeout).await {
                Ok(motd) => {
                    log::debug!(
                        "Successfully fetched MOTD information from source {}: {:?}",
                        source,
                        motd
                    );
                    let mut w = self.last_motd.write().await;
                    *w = Some(motd);
                }
                Err(err) => {
                    log::warn!(
                        "Could not fetch MOTD information from source {}: {:?}",
                        source,
                        err
                    );
                }
            }
        }
    }
}
