use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{RwLock, Semaphore},
    task::JoinSet,
};

use crate::{config::RuntimeConfigProvider, BackendServer, BackendState};

/// Health information about a backend server.
#[derive(Debug, Default, Clone)]
pub struct ServerHealth {
    /// Whether the server is accessible and well.
    pub alive: bool,
    /// Whether the server was ever alive.
    pub ever_alive: bool,
    /// Number of failed ping attempts in a row.
    pub failed_attempts: usize,
}

/// Controller overseeing the health of a backend.
pub struct HealthController {
    execute_lock: Semaphore,

    // Runtime config provider.
    config_provider: Arc<RuntimeConfigProvider>,
    /// Backend state.
    backend_state: Arc<RwLock<BackendState>>,
}

impl HealthController {
    pub fn new(
        config_provider: Arc<RuntimeConfigProvider>,
        backend_state: Arc<RwLock<BackendState>>,
    ) -> Self {
        Self {
            execute_lock: Semaphore::new(1),
            config_provider,
            backend_state,
        }
    }

    /// Executes a health check of all servers.
    /// Stale servers that have finished being used will be removed here too.
    pub async fn execute(&self) {
        let _permit = self.execute_lock.acquire();
        let local_addr = {
            let config = self.config_provider.read().await;
            config.proxy_bind.clone()
        };
        let mut join_set = JoinSet::new();
        {
            let mut backend_state = self.backend_state.write().await;
            backend_state
                .known_servers
                .retain(|server| server.upgrade().is_some());
            for weak_ref in backend_state.known_servers.iter() {
                let server = match weak_ref.upgrade() {
                    Some(server) => server,
                    None => continue,
                };
                let local_addr = local_addr.clone();
                join_set.spawn(async move {
                    HealthController::check_health(local_addr, server).await;
                });
            }
        }
        log::debug!("Checking health of {} backend servers...", join_set.len());
        loop {
            if join_set.join_next().await.is_none() {
                break;
            }
        }
    }

    /// Performs a health check on a server.
    async fn check_health(local_addr: String, server: Arc<BackendServer>) {
        let timeout = Duration::from_secs(5);
        let success = raknet::bedrock::ping(
            &local_addr,
            &server.addr,
            server.use_proxy_protocol().await,
            timeout,
        )
        .await
        .is_ok();
        let mut server_state = server.state.write().await;
        let health = &mut server_state.health;
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
        drop(server_state);
        if prev_alive != alive {
            if alive {
                log::info!("Backend server {} is now alive", &server.addr);
            } else {
                log::warn!("Backend server {} seems unreachable", &server.addr);
            }
        }
    }
}
