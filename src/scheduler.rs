use std::{sync::Arc, time::Duration};

use tokio::sync::{Notify, Semaphore};

use crate::{config::ConfigProvider, health::HealthController, motd::MOTDReflector};

/// A [`Scheduler`] is responsible for handling repeating tasks.
/// Used for [`crate::motd::MOTDReflector`] and [`crate::health::HealthController`].
pub struct Scheduler(Arc<Internals>);

struct Internals {
    lock: Semaphore,
    stop_notify: Notify,

    config_provider: Arc<ConfigProvider>,

    motd_reflector: Arc<MOTDReflector>,
    health_controller: Arc<HealthController>,
}

impl Scheduler {
    pub fn new(
        config_provider: Arc<ConfigProvider>,
        motd_reflector: Arc<MOTDReflector>,
        health_controller: Arc<HealthController>,
    ) -> Self {
        let internals = Internals {
            lock: Semaphore::new(1),
            stop_notify: Notify::new(),
            config_provider,
            motd_reflector,
            health_controller,
        };
        Self(Arc::new(internals))
    }

    /// Checks whether the scheduler is currently running.
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

impl Internals {
    async fn run(&self) -> anyhow::Result<()> {
        let (motd_rate, health_check_rate) = {
            let config = self.config_provider.read().await;
            let motd_rate = Duration::from_secs(u64::max(config.backend.motd_refresh_rate, 1));
            let health_check_rate =
                Duration::from_secs(u64::max(config.backend.health_check_rate, 1));
            (motd_rate, health_check_rate)
        };
        let mut motd_interval = tokio::time::interval(motd_rate);
        let mut health_check_interval = tokio::time::interval(health_check_rate);
        loop {
            tokio::select! {
                _ = self.stop_notify.notified() => return Ok(()),

                _ = motd_interval.tick() => {
                    tokio::spawn({
                        let motd_reflector = self.motd_reflector.clone();
                        async move { motd_reflector.execute().await }
                    });
                },
                _ = health_check_interval.tick() => {
                    tokio::spawn({
                        let health_controller = self.health_controller.clone();
                        async move { health_controller.execute().await }
                    });
                },
            }
        }
    }
}
