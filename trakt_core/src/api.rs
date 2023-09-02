use std::sync::Arc;

use trakt_api::model;
use trakt_api::provider::{NodeError, TraktApi};
use uuid::Uuid;

use crate::{Backend, BackendPlatform, BackendServer, ProxyServer};

/// Trait to convert internal model representation to API model.
///
/// Converting internal models to API models is not as
/// straightforward as it would need to be to make use of
/// the builtin [`From`] trait (async needed).
#[async_trait::async_trait]
pub trait IntoApiModel {
    type Model;

    /// Turns into an API model.
    ///
    /// ## Arguments
    ///
    /// * `hydrate` - When relevant, how many "layers" to hydrate
    async fn into_api_model(&self, hydrate: usize) -> Self::Model;
}

#[async_trait::async_trait]
impl IntoApiModel for Backend {
    type Model = model::Backend;

    async fn into_api_model(&self, hydrate: usize) -> Self::Model {
        let game_edition = match &self.platform {
            BackendPlatform::Bedrock { .. } => model::GameEdition::Bedrock,
        };
        let state = self.state.read().await;
        let servers = if hydrate > 0 {
            let mut servers = Vec::with_capacity(state.known_servers.len());
            let next_hydrate = hydrate.saturating_sub(1);
            for weak_ref in state.known_servers.iter() {
                let server = match weak_ref.upgrade() {
                    Some(server) => server,
                    None => continue,
                };
                servers.push(server.into_api_model(next_hydrate).await);
            }
            Some(servers)
        } else {
            None
        };
        model::Backend {
            uid: self.uid,
            name: self.id.clone(),
            game_edition,
            servers,
        }
    }
}

#[async_trait::async_trait]
impl IntoApiModel for BackendServer {
    type Model = model::Server;

    async fn into_api_model(&self, _hydrate: usize) -> Self::Model {
        let state = self.state.read().await;
        let health = model::ServerHealth {
            alive: state.health.alive,
            ever_alive: state.health.ever_alive,
        };
        let player_count = state.connected_players.len();
        model::Server {
            uid: self.uid,
            address: self.addr.to_string(),
            proxy_protocol: state.proxy_protocol,
            status: model::ServerStatus::Active,
            health,
            load_score: state.load_score,
            player_count,
        }
    }
}

/// Single-node API provider from a proxy server.
///
/// [`TraktApi`] implementation for a [`ProxyServer`].
pub struct SingleProxyApi<S: ProxyServer> {
    node_uid: Uuid,
    node_name: String,
    proxy_server: Arc<S>,
}

impl<S> SingleProxyApi<S>
where
    S: ProxyServer,
{
    pub fn new<N: ToString>(node_name: N, proxy_server: Arc<S>) -> Self {
        Self {
            node_uid: Uuid::new_v4(),
            node_name: node_name.to_string(),
            proxy_server,
        }
    }

    async fn node(&self, hydrate: bool) -> model::Node {
        let backends = if hydrate {
            let backends = self.proxy_server.get_backends().await;
            let mut models = Vec::with_capacity(backends.len());
            for backend in backends.into_iter() {
                models.push(backend.into_api_model(1).await);
            }
            Some(models)
        } else {
            None
        };
        model::Node {
            uid: self.node_uid,
            name: self.node_name.clone(),
            backends,
        }
    }
}

#[async_trait::async_trait]
impl<S> TraktApi for SingleProxyApi<S>
where
    S: ProxyServer,
{
    async fn get_nodes(&self, hydrate: bool) -> Vec<Result<model::Node, NodeError>> {
        vec![Ok(self.node(hydrate).await)]
    }

    async fn get_node(&self, node_uid: &Uuid) -> Result<Option<model::Node>, NodeError> {
        if self.node_uid.eq(node_uid) {
            Ok(Some(self.node(true).await))
        } else {
            Ok(None)
        }
    }

    async fn get_backend(
        &self,
        node_uid: &Uuid,
        backend_uid: &Uuid,
        hydrate: bool,
    ) -> Result<Option<model::Backend>, NodeError> {
        if self.node_uid.eq(node_uid) {
            let backend = self.proxy_server.get_backend(backend_uid).await;
            let hydrate_level = hydrate as usize;
            match backend {
                Some(backend) => Ok(Some(backend.into_api_model(hydrate_level).await)),
                None => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    async fn get_server(
        &self,
        node_uid: &Uuid,
        backend_uid: &Uuid,
        server_uid: &Uuid,
    ) -> Result<Option<model::Server>, NodeError> {
        if self.node_uid.eq(node_uid) {
            let backend = match self.proxy_server.get_backend(backend_uid).await {
                Some(backend) => backend,
                None => return Ok(None),
            };
            let backend_state = backend.state.read().await;
            let server = backend_state
                .known_servers
                .iter()
                .filter_map(|weak_ref| weak_ref.upgrade())
                .find(|server| server.uid.eq(server_uid));
            match server {
                Some(server) => Ok(Some(server.into_api_model(0).await)),
                None => Ok(None),
            }
        } else {
            Ok(None)
        }
    }
}
