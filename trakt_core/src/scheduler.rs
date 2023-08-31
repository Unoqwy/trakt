use std::{sync::Arc, time::Duration};

use tokio::sync::{Notify, Semaphore};

use crate::{config::RuntimeConfigProvider, BackendPlatform, ProxyServer};

/// A [`Scheduler`] is responsible for handling repeating tasks.
/// Used for health checks and MOTD caching.
pub struct Scheduler<S>(Arc<Internals<S>>);

struct Internals<S> {
    lock: Semaphore,
    stop_notify: Notify,

    /// Runtime config provider.
    config_provider: Arc<RuntimeConfigProvider>,

    /// Proxy server.
    proxy_server: Arc<S>,
}

impl<S: ProxyServer + 'static> Scheduler<S> {
    pub fn new(config_provider: Arc<RuntimeConfigProvider>, proxy_server: Arc<S>) -> Self {
        let internals = Internals {
            lock: Semaphore::new(1),
            stop_notify: Notify::new(),
            config_provider,
            proxy_server,
        };
        Self(Arc::new(internals))
    }

    pub fn is_running(&self) -> bool {
        self.0.lock.available_permits() == 0
    }

    /// Starts the scheduler.
    pub fn start(&self) {
        if self.is_running() {
            return;
        }
        let inner = self.0.clone();
        tokio::spawn(async move {
            let _permit = inner.lock.acquire().await;
            if let Err(err) = inner.run().await {
                log::error!("Scheduler stopped with an error: {:?}", err);
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
        self.0.stop_notify.notify_one();
        if wait {
            let _ = self.0.lock.acquire().await;
        }
    }

    /// Restarts the scheduler.
    ///
    /// This is useful for config changes to take effect after a reload.
    pub async fn restart(&self) {
        self.stop(true).await;
        self.start()
    }
}

impl<S: ProxyServer + 'static> Internals<S> {
    async fn run(&self) -> anyhow::Result<()> {
        let (motd_rate, health_check_rate) = {
            let config = self.config_provider.read().await;
            let motd_rate = Duration::from_secs(u64::max(config.motd_refresh_rate, 1));
            let health_check_rate = Duration::from_secs(u64::max(config.health_check_rate, 1));
            (motd_rate, health_check_rate)
        };
        let mut motd_interval = tokio::time::interval(motd_rate);
        let mut health_check_interval = tokio::time::interval(health_check_rate);
        loop {
            tokio::select! {
                _ = self.stop_notify.notified() => return Ok(()),

                _ = motd_interval.tick() => {
                    tokio::spawn({
                        let proxy_server = self.proxy_server.clone();
                        async move { Internals::update_motd(proxy_server).await }
                    });
                },
                _ = health_check_interval.tick() => {
                    tokio::spawn({
                        let proxy_server = self.proxy_server.clone();
                        async move { Internals::check_health(proxy_server).await }
                    });
                },
            }
        }
    }

    async fn update_motd(proxy_server: Arc<S>) {
        for backend in proxy_server.get_backends().await {
            tokio::spawn(async move {
                match &backend.platform {
                    BackendPlatform::Bedrock { motd_cache, .. } => {
                        motd_cache.update().await;
                    }
                }
            });
        }
    }

    async fn check_health(proxy_server: Arc<S>) {
        for backend in proxy_server.get_backends().await {
            tokio::spawn(async move {
                backend.health_controller.execute().await;
            });
        }
    }
}
