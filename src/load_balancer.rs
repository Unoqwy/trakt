use std::collections::HashSet;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use tokio::sync::Mutex;

use crate::config::{self, ConfigProvider};

/// The load balancer is responsible for picking the backend to point
/// new connections to. It also keeps track of the the health of backends.
pub struct LoadBalancer {
    /// Config provider.
    config_provider: Arc<ConfigProvider>,

    /// Inner state.
    state: Mutex<LoadBalancerExclusiveState>,
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
    /// TODO: Current health status.
    pub health: RwLock<ServerHealth>,
    /// Current number of clients assigned to that server.
    pub load: AtomicUsize,
}

/// Health information about a backend server.
#[derive(Debug, Default)]
pub struct ServerHealth {
    /// Whether the server is accessible and well.
    pub alive: bool,
}

impl LoadBalancer {
    /// Initializes a load balancer from config.
    ///
    /// ## Arguments
    ///
    /// * `config_provider` - Config provider
    pub async fn init(config_provider: Arc<ConfigProvider>) -> Self {
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
        let algo_reset = match (&state.algo, &new_method) {
            (LoadBalanceAlgorithm::RoundRobin { .. }, config::LoadBalanceMethod::RoundRobin) => {
                false
            }
            (LoadBalanceAlgorithm::LeastConnected, config::LoadBalanceMethod::LeastConnected) => {
                false
            }
            _ => true,
        };
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
            state.servers.push(Arc::new(BackendServer {
                addr,
                health: RwLock::new(ServerHealth::default()),
                load: AtomicUsize::new(0),
            }));
            new_count += 1;
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

    /// Gets the next backend server according to the load balancing method.
    ///
    /// Will return [`None`] if no server is available.
    pub async fn next(&self) -> Option<Arc<BackendServer>> {
        let mut state = self.state.lock().await;
        let server_count = state.servers.len();
        if server_count == 0 {
            return None;
        }
        match &state.algo {
            LoadBalanceAlgorithm::RoundRobin { index } => {
                let index = *index;
                match &mut state.algo {
                    LoadBalanceAlgorithm::RoundRobin { index } => {
                        if *index + 1 % server_count == 0 {
                            *index = 0;
                        } else {
                            *index += 1;
                        }
                    }
                    _ => unreachable!(),
                };
                state.servers.get(index).cloned()
            }
            LoadBalanceAlgorithm::LeastConnected => {
                let mut min_load = 0;
                let mut target = None;
                for server in state.servers.iter() {
                    let load = server.load.load(Ordering::Acquire);
                    if load < min_load {
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
