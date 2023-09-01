use std::{
    collections::HashSet,
    net::SocketAddr,
    str::FromStr,
    sync::{
        Arc, Weak,
    },
};

use rand::Rng;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    bedrock::BedrockMotdCache,
    config::{BackendConfig, RuntimeConfigProvider},
    HealthController, LoadBalancer, ServerHealth,
};

/// A set of servers that a [`crate::proxy::Proxy`] can
/// route players to.
///
/// Each backend has its own load balancer to decide
/// where to send new connections to.
pub struct Backend {
    /// Unique ID (not persistent across restart).
    pub uid: Uuid,
    /// Backend local ID.
    pub id: String,
    /// Health controller.
    pub health_controller: HealthController,
    /// Load balancer.
    pub load_balancer: Box<dyn LoadBalancer>,
    /// Mutable state. Some services can keep
    /// a reference for convenient access.
    pub state: Arc<RwLock<BackendState>>,
    /// Platform information.
    pub platform: BackendPlatform,
}

/// Mutable state of a [`Backend`].
#[derive(Debug, Clone, Default)]
pub struct BackendState {
    /// MOTD sources. If empty, backend servers will be used instead.
    pub motd_sources: Vec<MotdSource>,
    /// Current backend servers. More may exist if the config reloaded
    /// removing backends but some clients are still connected to it.
    pub servers: Vec<Arc<BackendServer>>,
    /// Known backend servers. This may include stale servers that are
    /// no longer used by the load balancer but still have players connected.
    pub known_servers: Vec<Weak<BackendServer>>,
}

/// Platform-specific backend state.
pub enum BackendPlatform {
    Bedrock {
        /// MOTD cache.
        motd_cache: BedrockMotdCache,
        /// Random ID representing the backend. May change at will,
        /// but should not be the same across several backends.
        server_uuid: i64,
    },
}

/// A [`BackendServer`] is a Minecraft server
/// to which traffic can be routed to.
#[derive(Debug)]
pub struct BackendServer {
    /// Unique ID (not persistent across restart).
    pub uid: Uuid,
    /// Remote address of the server.
    pub addr: SocketAddr,
    /// Mutable state.
    pub state: RwLock<BackendServerState>,
}

/// Mutable state of a [`BackendServer`].
#[derive(Debug, Clone, Default)]
pub struct BackendServerState {
    /// Whether the remote server supports proxy protocol.
    pub proxy_protocol: bool,
    /// Server health.
    pub health: ServerHealth,
    /// Load score.
    pub load_score: usize,
    /// Online players.
    pub connected_players: HashSet<SocketAddr>,
}

/// A [`MotdSource`] is similar to a [`BackendServer`],
/// excepts it is meant to fetch MOTD information from,
/// not connect players to.
#[derive(Debug, Clone)]
pub struct MotdSource {
    /// Remote address of the server.
    pub addr: SocketAddr,
    /// Whether the remote server supports proxy protocol.
    pub proxy_protocol: bool,
}

/// Result of a backend reload.
#[derive(Debug, Clone, Default)]
pub struct BackendLoadResult {
    /// Whether that was a reload.
    pub reload: bool,
    /// Number of active servers.
    pub server_count: usize,
    /// Number of newly active servers.
    pub new_count: usize,
    /// Number of removed servers.
    pub removed_count: usize,
}

impl Backend {
    /// Initializes a new empty backend for Bedrock Edition.
    ///
    /// ## Arguments
    ///
    /// * `id` - Backend ID
    /// * `load_balancer_fn` - Load balancer producer
    /// * `config_provider` - Runtime config provider
    /// * `backend_config` - Config to initialize the backend with
    pub async fn new_bedrock<F>(
        id: String,
        load_balancer_fn: F,
        config_provider: Arc<RuntimeConfigProvider>,
        backend_config: Option<&BackendConfig>,
    ) -> (Self, BackendLoadResult)
    where
        F: FnOnce(Arc<RwLock<BackendState>>) -> Box<dyn LoadBalancer>,
    {
        let mut state: BackendState = Default::default();
        let load_result = match backend_config {
            Some(config) => state.load_config(config, false).await,
            None => BackendLoadResult::default(),
        };
        let state = Arc::new(RwLock::new(state));
        let load_balancer = load_balancer_fn(state.clone());
        let health_controller = HealthController::new(config_provider.clone(), state.clone());
        let motd_cache = BedrockMotdCache::new(config_provider, state.clone());
        let server_uuid = rand::thread_rng().gen();
        let platform = BackendPlatform::Bedrock {
            motd_cache,
            server_uuid,
        };
        let backend = Self {
            uid: Uuid::new_v4(),
            id,
            health_controller,
            load_balancer,
            state,
            platform,
        };
        (backend, load_result)
    }
}

impl BackendServer {
    pub fn new(addr: SocketAddr, proxy_protocol: bool) -> Self {
        let mut state: BackendServerState = Default::default();
        state.proxy_protocol = proxy_protocol;
        Self {
            uid: Uuid::new_v4(),
            addr,
            state: RwLock::new(state),
        }
    }

    /// Returns whether the remote server uses proxy protocol.
    pub async fn use_proxy_protocol(&self) -> bool {
        let state = self.state.read().await; 
        state.proxy_protocol
    }

    /// Returns whether the server's health is alive.
    pub async fn is_alive(&self) -> bool {
        let state = self.state.read().await; 
        state.health.alive
    }

    /// Modifies the load score by a delta.
    ///
    /// This uses saturating operations to ensure it never overflows
    pub async fn modify_load(&self, delta: isize) {
        let mut state = self.state.write().await; 
        if delta >= 0 {
            state.load_score = state.load_score.saturating_add(delta as usize);
        } else {
            state.load_score = state.load_score.saturating_sub(-delta as usize);
        }
    }
}

impl Backend {
    /// Reloads the backend configuration, including the servers.
    ///
    /// ## Arguments
    ///
    /// * `backend_config` - Backend configuration
    pub async fn reload_config(&self, backend_config: &BackendConfig) -> BackendLoadResult {
        let mut state = self.state.write().await;
        state.load_config(backend_config, true).await
    }
}

impl BackendState {
    /// Returns configured MOTD sources or default to
    /// active backend servers.
    pub async fn motd_sources_or_default(&self) -> Vec<MotdSource> {
        if self.motd_sources.is_empty() {
            let mut sources = Vec::with_capacity(self.servers.len());
            for server in self.servers.iter() {
                let source = MotdSource {
                    addr: server.addr,
                    proxy_protocol: server.use_proxy_protocol().await,
                };
                sources.push(source);
            }
            sources
        } else {
            self.motd_sources.clone()
        }
    }

    /// Registers a new server.
    ///
    /// ## Arguments
    ///
    /// * `server` - Server
    /// * `stale` - Whether the server is already stale
    pub fn register_server(&mut self, server: Arc<BackendServer>, stale: bool) {
        self.known_servers.push(Arc::downgrade(&server));
        if !stale {
            self.servers.push(server);
        }
    }

    /// Gets an active backend server for a given adrress.
    ///
    /// If the server exists but is stale, it will return [`None`].
    ///
    /// ## Arguments
    ///
    /// * `addr` - Server address
    pub fn get_server(&self, addr: SocketAddr) -> Option<Arc<BackendServer>> {
        self.servers
            .iter()
            .find(|server| server.addr.eq(&addr))
            .cloned()
    }

    /// (Re)loads the servers from configuration.
    ///
    /// ## Arguments
    ///
    /// * `backend_config` - Backend configuration
    /// * `reload` - Whether this is a reload
    pub async fn load_config(
        &mut self,
        backend_config: &BackendConfig,
        reload: bool,
    ) -> BackendLoadResult {
        let mut new_count = 0;
        let mut seen: HashSet<SocketAddr> = HashSet::new();
        for server_config in backend_config.servers.iter() {
            let addr = match SocketAddr::from_str(&server_config.address) {
                Ok(addr) => addr,
                Err(err) => {
                    log::error!(
                        "Could not load configured backend server with address {}: {:?}",
                        server_config.address,
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
            let proxy_protocol = server_config
                .proxy_protocol
                .unwrap_or(backend_config.proxy_protocol);
            let active = self.servers.iter_mut().find(|server| server.addr.eq(&addr));
            if let Some(active) = active {
                let mut active_state = active.state.write().await;
                active_state.proxy_protocol = proxy_protocol;
                continue;
            }
            let server = Arc::new(BackendServer::new(addr, proxy_protocol));
            new_count += 1;
            self.register_server(server, false);
        }
        let initial_count = self.servers.len();
        self.servers.retain(|server| seen.contains(&server.addr));
        let server_count = self.servers.len();
        let removed_count = initial_count - server_count;
        let reload = reload || removed_count > 0;
        BackendLoadResult {
            reload,
            server_count,
            new_count,
            removed_count,
        }
    }
}
