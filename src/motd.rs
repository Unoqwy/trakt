use std::{sync::Arc, time::Duration};

use tokio::{
    sync::{Notify, RwLock, Semaphore},
    time::{self, MissedTickBehavior},
};

use crate::{
    config::ConfigProvider,
    raknet::ping::{self, MOTD},
};

/// A controller that periodically fetches MOTD information
/// from the backend and exposes the last successful response.
pub struct MOTDReflector {
    scheduler_lock: Semaphore,
    execute_lock: Semaphore,
    stop_notify: Notify,

    /// Config provider.
    config_provider: Arc<ConfigProvider>,

    /// Last successful MOTD response, if any.
    last_motd: RwLock<Option<MOTD>>,
}

impl MOTDReflector {
    pub fn new(config_provider: Arc<ConfigProvider>) -> Arc<Self> {
        Arc::new(Self {
            scheduler_lock: Semaphore::new(1),
            execute_lock: Semaphore::new(1),
            stop_notify: Notify::new(),
            config_provider,
            last_motd: RwLock::new(None),
        })
    }

    /// Checks whether the scheduler is currently running.
    pub fn is_running(&self) -> bool {
        self.scheduler_lock.available_permits() == 0
    }

    /// Starts the scheduler.
    pub fn start(self: Arc<Self>) {
        if self.is_running() {
            return;
        }
        tokio::spawn(async move {
            if let Err(err) = self.run().await {
                log::error!("MOTD reflector stopped with an error: {:?}", err);
            }
        });
    }

    /// Stops the scheduler.
    ///
    /// ## Arguments
    ///
    /// * `wait` - Whether to wait until the scheduler has actually stopped.
    pub async fn stop(&self, wait: bool) {
        if !self.is_running() {
            return;
        }
        self.stop_notify.notify_one();
        if wait {
            let _ = self.scheduler_lock.acquire().await;
        }
    }

    /// Returns a clone of the last sucessful MOTD information received.
    pub async fn last_motd(&self) -> Option<MOTD> {
        self.last_motd.read().await.clone()
    }

    /// Runs the scheduler.
    async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        let refresh_rate = {
            let config = self.config_provider.read().await;
            Duration::from_secs(u64::max(config.backend.motd_refresh_rate, 1))
        };
        let mut interval = time::interval(refresh_rate);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let _ = self.scheduler_lock.acquire().await;
        loop {
            tokio::select! {
                _ = self.stop_notify.notified() => return Ok(()),

                _ = interval.tick() => self.execute().await,
            }
        }
    }

    /// Fetches the MOTD.
    pub async fn execute(&self) {
        let _ = self.scheduler_lock.acquire().await;
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
        log::trace!(
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
