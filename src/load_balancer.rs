use std::collections::HashSet;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::config::{self, ConfigProvider};
use crate::health::{HealthController, ServerHealth};

/// The load balancer is responsible for picking the backend to point
/// new connections to. It also keeps track of the the health of backends.
pub struct LoadBalancer {
    /// Config provider.
    config_provider: Arc<ConfigProvider>,

    /// Inner state.
    state: Mutex<LoadBalancerExclusiveState>,

    health_controller: Arc<HealthController>,
}

/// Load balancer state that requires exclusive access (achieved with a mutex).
#[derive(Debug)]
struct LoadBalancerExclusiveState {
    /// Load balancing algorithm, accompanied by its own state, if any.
    algo: LoadBalanceAlgorithm,
    /// Current backend servers. More may exist if the config reloaded removing backends but
    /// some clients are still connected to it.
    servers: Vec<Arc<BackendServer>>,
}

#[derive(Debug, Clone)]
enum LoadBalanceAlgorithm {
    RoundRobin { index: usize },
    LeastConnected,
}

/// A [`BackendServer`] is a Minecraft Bedrock Edition server/proxy
/// to which traffic can be routed to.
#[derive(Debug)]
pub struct BackendServer {
    /// Remote address of the server.
    pub addr: SocketAddr,
    /// Current health status.
    pub health: RwLock<ServerHealth>,
    /// Current number of clients assigned to that server.
    pub load: AtomicUsize,
}

impl BackendServer {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            health: RwLock::new(ServerHealth::default()),
            load: AtomicUsize::new(0),
        }
    }
}

impl LoadBalancer {
    /// Initializes a load balancer from config.
    ///
    /// ## Arguments
    ///
    /// * `config_provider` - Config provider
    /// * `health_controller` - Health Controller
    pub async fn init(
        config_provider: Arc<ConfigProvider>,
        health_controller: Arc<HealthController>,
    ) -> Self {
        let method = {
            let config = config_provider.read().await;
            config
                .load_balance_method
                .unwrap_or(config::LoadBalanceMethod::RoundRobin)
        };
        let algo = LoadBalanceAlgorithm::init(method);
        let state = LoadBalancerExclusiveState {
            algo,
            servers: Vec::new(),
        };
        let __self = Self {
            config_provider,
            state: Mutex::new(state),
            health_controller,
        };
        __self.load_config(false).await;
        __self
    }

    /// Reloads configuration.
    #[inline]
    pub async fn reload_config(&self) {
        self.load_config(true).await;
    }

    /// Loads servers from configuration.
    ///
    /// ## Arguments
    ///
    /// * `reload` - Whether this is a reload
    async fn load_config(&self, reload: bool) {
        let config = self.config_provider.read().await;
        let mut state = self.state.lock().await;
        let new_method = config
            .load_balance_method
            .unwrap_or(config::LoadBalanceMethod::RoundRobin);
        let algo_reset = !matches!(
            (&state.algo, &new_method),
            (
                LoadBalanceAlgorithm::RoundRobin { .. },
                config::LoadBalanceMethod::RoundRobin
            ) | (
                LoadBalanceAlgorithm::LeastConnected,
                config::LoadBalanceMethod::LeastConnected
            )
        );
        if algo_reset {
            state.algo = LoadBalanceAlgorithm::init(new_method);
        }
        let mut new_count = 0;
        let mut seen: HashSet<SocketAddr> = HashSet::new();
        for config_server in config.backend.servers.iter() {
            let addr = match SocketAddr::from_str(&config_server.address) {
                Ok(addr) => addr,
                Err(err) => {
                    log::error!(
                        "Could not load configured backend server with address {}: {:?}",
                        config_server.address,
                        err
                    );
                    continue;
                }
            };
            if !seen.insert(addr) {
                log::warn!(
                    "Duplicate backend server pointing to {} in configuration",
                    addr
                );
                continue;
            }
            let active = state.servers.iter().find(|server| server.addr.eq(&addr));
            if active.is_some() {
                continue;
            }
            let server = Arc::new(BackendServer::new(addr));
            state.servers.push(server.clone());
            new_count += 1;
            self.health_controller.register_server(server).await;
        }
        let server_count = state.servers.len();
        state.servers.retain(|server| seen.contains(&server.addr));
        let removed_count = server_count - state.servers.len();
        if reload || removed_count > 0 {
            log::info!(
                "Reloaded load balancer. There are now {} backend servers ({} added, {} removed)",
                state.servers.len(),
                new_count,
                removed_count
            );
        } else {
            log::info!("Loaded {} backend servers", new_count);
        }
    }

    /// Gets an active backend server for a given adrress.
    ///
    /// If the server exists but is stale (load balancer doesn't know it),
    /// it will return [`None`].
    ///
    /// ## Arguments
    ///
    /// * `addr` - Server address
    pub async fn get_server(&self, addr: SocketAddr) -> Option<Arc<BackendServer>> {
        let state = self.state.lock().await;
        let active = state.servers.iter().find(|server| server.addr.eq(&addr));
        active.cloned()
    }

    /// Gets the next backend server according to the load balancing method.
    ///
    /// Will return [`None`] if no server is available.
    pub async fn next(&self) -> Option<Arc<BackendServer>> {
        let mut state = self.state.lock().await;
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
                let health = server.health.read().await;
                if health.alive {
                    alive_count += 1;
                }
            }
            alive_count > 0
        };
        log::debug!(
            "Getting next server from load balancer (algo: {:?}, respect_alive_status: {})",
            &state.algo,
            respect_alive_status
        );
        match &state.algo {
            LoadBalanceAlgorithm::RoundRobin { .. } => {
                for _ in 0..server_count {
                    let index = match &mut state.algo {
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
                            let health = server.health.read().await;
                            if !health.alive {
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
                    let load = server.load.load(Ordering::Acquire);
                    if load < min_load {
                        if respect_alive_status {
                            let health = server.health.read().await;
                            if !health.alive {
                                continue;
                            }
                        }
                        min_load = load;
                        target = Some(server.clone());
                    }
                }
                target
            }
        }
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
}
