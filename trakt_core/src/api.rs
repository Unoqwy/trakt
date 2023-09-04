use std::sync::Arc;

use trakt_api::constraint::Constraint;
use trakt_api::provider::{NodeError, TraktApi};
use trakt_api::{model, HydrateOptions};
use trakt_api::{BackendRefPath, ResourceRef, ServerRefPath};
use uuid::Uuid;

use crate::{Backend, BackendPlatform, BackendServer, ProxyServer};

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

    fn matches_ref(&self, node_ref: &ResourceRef) -> bool {
        match node_ref {
            ResourceRef::Uid(uid) => self.node_uid.eq(uid),
            ResourceRef::Name(name) => self.node_name.eq(name),
        }
    }

    async fn node(&self, hydrate_opts: HydrateOptions) -> model::Node {
        let backends = if hydrate_opts.node_backends {
            let backends = self.proxy_server.get_backends().await;
            let mut models = Vec::with_capacity(backends.len());
            for backend in backends.into_iter() {
                models.push(serialize_backend(&backend, hydrate_opts).await);
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

    async fn find_server(&self, server_path: &ServerRefPath) -> Option<Arc<BackendServer>> {
        if self.matches_ref(&server_path.node) {
            let backend = match self.proxy_server.get_backend(&server_path.backend).await {
                Some(backend) => backend,
                None => return None,
            };
            let backend_state = backend.state.read().await;
            let mut iter = backend_state
                .known_servers
                .iter()
                .filter_map(|weak_ref| weak_ref.upgrade());
            match &server_path.server {
                ResourceRef::Uid(uid) => iter.find(|server| server.uid.eq(uid)),
                ResourceRef::Name(name) => iter.find(|server| server.addr.to_string().eq(name)),
            }
        } else {
            None
        }
    }
}

#[async_trait::async_trait]
impl<S> TraktApi for SingleProxyApi<S>
where
    S: ProxyServer,
{
    async fn get_nodes(&self, hydrate_opts: HydrateOptions) -> Vec<Result<model::Node, NodeError>> {
        vec![Ok(self.node(hydrate_opts).await)]
    }

    async fn get_node(
        &self,
        node_ref: &ResourceRef,
        hydrate_opts: HydrateOptions,
    ) -> Result<Option<model::Node>, NodeError> {
        if self.matches_ref(node_ref) {
            Ok(Some(self.node(hydrate_opts).await))
        } else {
            Ok(None)
        }
    }

    async fn get_backend(
        &self,
        backend_path: &BackendRefPath,
        hydrate_opts: HydrateOptions,
    ) -> Result<Option<model::Backend>, NodeError> {
        if self.matches_ref(&backend_path.node) {
            let backend = self.proxy_server.get_backend(&backend_path.backend).await;
            match backend {
                Some(backend) => Ok(Some(serialize_backend(&backend, hydrate_opts).await)),
                None => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    async fn get_server(
        &self,
        server_path: &ServerRefPath,
        hydrate_opts: HydrateOptions,
    ) -> Result<Option<model::Server>, NodeError> {
        match self.find_server(server_path).await {
            Some(server) => Ok(Some(serialize_server(&server, hydrate_opts).await)),
            None => Ok(None),
        }
    }

    async fn clear_server_constraints(&self, server_path: &ServerRefPath) -> Result<(), NodeError> {
        if let Some(server) = self.find_server(server_path).await {
            let mut state = server.state.write().await;
            state.constraints.clear_all();
        }
        Ok(())
    }

    async fn set_server_constraint(
        &self,
        server_path: &ServerRefPath,
        key: &str,
        constraint: Option<Constraint>,
    ) -> Result<(), NodeError> {
        if let Some(server) = self.find_server(server_path).await {
            let mut state = server.state.write().await;
            state.constraints.set(key, constraint);
        }
        Ok(())
    }
}

pub async fn serialize_backend(backend: &Backend, hydrate_opts: HydrateOptions) -> model::Backend {
    let game_edition = match &backend.platform {
        BackendPlatform::Bedrock { .. } => model::GameEdition::Bedrock,
    };
    let state = backend.state.read().await;
    let servers = if hydrate_opts.backend_servers {
        let mut servers = Vec::with_capacity(state.known_servers.len());
        for weak_ref in state.known_servers.iter() {
            let server = match weak_ref.upgrade() {
                Some(server) => server,
                None => continue,
            };
            servers.push(serialize_server(&server, hydrate_opts).await);
        }
        Some(servers)
    } else {
        None
    };
    model::Backend {
        uid: backend.uid,
        name: backend.id.clone(),
        game_edition,
        servers,
    }
}

pub async fn serialize_server(
    server: &BackendServer,
    hydrate_opts: HydrateOptions,
) -> model::Server {
    let state = server.state.read().await;
    let health = model::ServerHealth {
        alive: state.health.alive,
        ever_alive: state.health.ever_alive,
    };
    let player_count = state.connected_players.len();
    let constraints = if hydrate_opts.server_constraints {
        Some(state.constraints.serialize_to_map())
    } else {
        None
    };
    model::Server {
        uid: server.uid,
        address: server.addr.to_string(),
        proxy_protocol: state.proxy_protocol,
        status: model::ServerStatus::Active,
        health,
        load_score: state.load_score,
        player_count,
        constraints,
    }
}
