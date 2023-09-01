use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::{config, BackendServer, BackendState};

/// A load balancer is responsible for picking the server
/// to point new connections to for a backend on the proxy.
///
/// The load balancer also manages the state of known servers,
/// and should report new servers to the [`HealthController`].
#[async_trait::async_trait]
pub trait LoadBalancer: Send + Sync {
    /// Gets the next backend server according to the load balancing method.
    ///
    /// Will return [`None`] if no server is available.
    async fn next(&self) -> Option<Arc<BackendServer>>;

    /// Returns the currently used load balancing method.
    async fn get_method(&self) -> config::LoadBalanceMethod;
}

/// Default load balancer implementation.
/// Supports all of [`config::LoadBalanceMethod`].
///
/// Each backend is meant to have its own instance.
pub struct DefaultLoadBalancer {
    /// Backend state.
    backend_state: Arc<RwLock<BackendState>>,
    /// Load balancing algorithm, accompanied by its own state, if any.
    algo: Mutex<LoadBalanceAlgorithm>,
}

#[derive(Debug, Clone)]
enum LoadBalanceAlgorithm {
    RoundRobin { index: usize },
    LeastConnected,
}

impl DefaultLoadBalancer {
    /// Initializes a load balancer for a backend.
    ///
    /// ## Arguments
    ///
    /// * `backend_state` - Backend state
    /// * `method` - Load balancing method to use
    pub fn init(
        backend_state: Arc<RwLock<BackendState>>,
        method: config::LoadBalanceMethod,
    ) -> Self {
        let algo = LoadBalanceAlgorithm::init(method);
        Self {
            backend_state,
            algo: Mutex::new(algo),
        }
    }

    /// Sets load balancing method.
    ///
    /// Will be of no consequence if the method is already being used,
    /// there is no need to check beforehand.
    ///
    /// ## Arguments
    ///
    /// * `method` - Load balancing method to use
    pub async fn set_method(&self, method: config::LoadBalanceMethod) {
        let mut algo = self.algo.lock().await;
        let algo_reset = !matches!(
            (&*algo, &method),
            (
                LoadBalanceAlgorithm::RoundRobin { .. },
                config::LoadBalanceMethod::RoundRobin
            ) | (
                LoadBalanceAlgorithm::LeastConnected,
                config::LoadBalanceMethod::LeastConnected
            )
        );
        if algo_reset {
            *algo = LoadBalanceAlgorithm::init(method);
        }
    }
}

#[async_trait::async_trait]
impl LoadBalancer for DefaultLoadBalancer {
    async fn next(&self) -> Option<Arc<BackendServer>> {
        let mut algo = self.algo.lock().await;
        let state = self.backend_state.read().await;
        let server_count = state.servers.len();
        if server_count == 0 {
            return None;
        }
        // when all backend servers are marked as alive
        // it might be an issue specific to pings, hence we still
        // want to allow players to attempt joining even if health status is wrong
        let respect_alive_status = {
            let mut alive_count = 0;
            for server in state.servers.iter() {
                if server.is_alive().await {
                    alive_count += 1;
                }
            }
            alive_count > 0
        };
        log::debug!(
            "Getting next server from load balancer (algo: {:?}, respect_alive_status: {})",
            algo,
            respect_alive_status
        );
        match &*algo {
            LoadBalanceAlgorithm::RoundRobin { .. } => {
                for _ in 0..server_count {
                    let index = match &mut *algo {
                        LoadBalanceAlgorithm::RoundRobin { index } => {
                            let prev_index = *index;
                            if prev_index + 1 >= server_count {
                                *index = 0;
                            } else {
                                *index += 1;
                            }
                            prev_index
                        }
                        _ => unreachable!(),
                    };
                    match state.servers.get(index) {
                        Some(server) if respect_alive_status => {
                            if !server.is_alive().await {
                                continue;
                            }
                            return Some(server.clone());
                        }
                        Some(server) => return Some(server.clone()),
                        _ => {}
                    }
                }
                None
            }
            LoadBalanceAlgorithm::LeastConnected => {
                let mut min_load = usize::MAX;
                let mut target = None;
                for server in state.servers.iter() {
                    let state = server.state.read().await;
                    if state.load_score < min_load {
                        if respect_alive_status {
                            if !state.health.alive {
                                continue;
                            }
                        }
                        min_load = state.load_score;
                        target = Some(server.clone());
                    }
                }
                target
            }
        }
    }

    async fn get_method(&self) -> config::LoadBalanceMethod {
        let algo = self.algo.lock().await;
        algo.method()
    }
}

impl LoadBalanceAlgorithm {
    /// Initializes the algorithm and its state given a configured method.
    pub fn init(method: config::LoadBalanceMethod) -> Self {
        match method {
            config::LoadBalanceMethod::RoundRobin => Self::RoundRobin { index: 0 },
            config::LoadBalanceMethod::LeastConnected => Self::LeastConnected,
        }
    }

    pub fn method(&self) -> config::LoadBalanceMethod {
        match self {
            Self::RoundRobin { .. } => config::LoadBalanceMethod::RoundRobin,
            Self::LeastConnected { .. } => config::LoadBalanceMethod::LeastConnected,
        }
    }
}
