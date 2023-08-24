use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use tokio::{
    sync::{Mutex, Semaphore},
    task::JoinSet,
};

use crate::{config::ConfigProvider, load_balancer::BackendServer, raknet::ping};

/// Controller overseeing the health of all backend servers.
pub struct HealthController {
    execute_lock: Semaphore,

    /// Config provider.
    config_provider: Arc<ConfigProvider>,

    /// Knonwn backend servers. This may include stale servers that are
    /// no longer used by the load balancer.
    servers: Mutex<Vec<Weak<BackendServer>>>,
}

/// Health information about a backend server.
#[derive(Debug, Default)]
pub struct ServerHealth {
    /// Whether the server is accessible and well.
    pub alive: bool,
    /// Whether the server was ever alive.
    pub ever_alive: bool,
    /// Number of failed ping attempts in a row.
    pub failed_attempts: usize,
}

impl HealthController {
    pub fn new(config_provider: Arc<ConfigProvider>) -> Self {
        Self {
            execute_lock: Semaphore::new(1),
            config_provider,
            servers: Mutex::new(Vec::new()),
        }
    }

    /// Registers a server to start performing health checks on it.
    pub async fn register_server(&self, server: Arc<BackendServer>) {
        let mut servers = self.servers.lock().await;
        servers.push(Arc::downgrade(&server));
    }

    /// Executes a health check of all servers.
    /// Stale servers that have finished being used will be removed here too.
    pub async fn execute(&self) {
        let _permit = self.execute_lock.acquire();
        let (local_addr, proxy_protocol) = {
            let config = self.config_provider.read().await;
            let proxy_protocol = config.proxy_protocol.unwrap_or(true);
            (config.proxy_bind.clone(), proxy_protocol)
        };
        let mut servers = self.servers.lock().await;
        servers.retain(|server| server.upgrade().is_some());
        let mut join_set = JoinSet::new();
        for weak_ref in servers.iter() {
            let server = match weak_ref.upgrade() {
                Some(server) => server,
                None => continue,
            };
            let local_addr = local_addr.clone();
            join_set.spawn(async move {
                HealthController::check_health(local_addr, proxy_protocol, server).await;
            });
        }
        drop(servers);
        log::debug!("Checking health of {} backend servers...", join_set.len());
        loop {
            if join_set.join_next().await.is_none() {
                break;
            }
        }
    }

    /// Performs a health check on server.
    async fn check_health(local_addr: String, proxy_protocol: bool, server: Arc<BackendServer>) {
        let timeout = Duration::from_secs(5);
        let success = ping::ping(&local_addr, &server.addr, proxy_protocol, timeout)
            .await
            .is_ok();
        let mut health = server.health.write().await;
        let prev_alive = health.alive;
        if success {
            health.failed_attempts = 0;
            health.alive = true;
            health.ever_alive = true;
        } else {
            health.failed_attempts += 1;
            health.alive = health.ever_alive && health.failed_attempts < 3;
        }
        let alive = health.alive;
        drop(health);
        if prev_alive != alive {
            if alive {
                log::info!("Backend server {} is now alive", &server.addr);
            } else {
                log::warn!("Backend server {} seems unreachable", &server.addr);
            }
        }
    }
}
