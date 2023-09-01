use std::{sync::Arc, time::Duration};

use tokio::sync::{RwLock, Semaphore};

use raknet::bedrock::{ping, Motd};

use crate::{config::RuntimeConfigProvider, BackendState};

/// A controller that periodically fetches MOTD information
/// from the backend and exposes the last successful response.
pub struct BedrockMotdCache {
    update_lock: Semaphore,

    /// Runtime config provider.
    config_provider: Arc<RuntimeConfigProvider>,
    /// Backend state.
    backend_state: Arc<RwLock<BackendState>>,

    /// Last successful MOTD response, if any.
    last_motd: RwLock<Option<Motd>>,
}

impl BedrockMotdCache {
    pub fn new(
        config_provider: Arc<RuntimeConfigProvider>,
        backend_state: Arc<RwLock<BackendState>>,
    ) -> Self {
        Self {
            update_lock: Semaphore::new(1),
            config_provider,
            backend_state,
            last_motd: RwLock::new(None),
        }
    }

    /// Returns a clone of the last sucessful MOTD information received.
    pub async fn last_motd(&self) -> Option<Motd> {
        self.last_motd.read().await.clone()
    }

    /// Fetches MOTD information and updates the cache.
    pub async fn update(&self) {
        let _permit = self.update_lock.acquire().await;
        let local_addr = {
            let config = self.config_provider.read().await;
            config.proxy_bind.clone()
        };
        let sources = {
            let state = self.backend_state.read().await;
            state.motd_sources_or_default().await
        };
        log::debug!(
            "Fetching MOTD information from backend ({} sources)...",
            sources.len()
        );
        let timeout = Duration::from_secs(5);
        for source in sources.into_iter() {
            match ping(&local_addr, &source.addr, source.proxy_protocol, timeout).await {
                Ok(motd) => {
                    log::debug!(
                        "Successfully fetched MOTD information from source {}: {:?}",
                        source.addr,
                        motd
                    );
                    let mut w = self.last_motd.write().await;
                    *w = Some(motd);
                }
                Err(err) => {
                    log::warn!(
                        "Could not fetch MOTD information from source {}: {:?}",
                        source.addr,
                        err
                    );
                }
            }
        }
    }
}
