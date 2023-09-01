use trakt_api::model::{self, GameEdition};

use crate::{Backend, BackendPlatform, BackendServer};

/// Welcome.
///
/// Converting internal models to API models is not as
/// straightforward as it would need to be to make use of
/// the builtin [`From`] trait (async needed).
#[async_trait::async_trait]
pub trait IntoApiModel {
    type Model;

    async fn into_api_model(&self) -> Self::Model;
}

#[async_trait::async_trait]
impl IntoApiModel for Backend {
    type Model = model::Backend;

    async fn into_api_model(&self) -> Self::Model {
        let game_edition = match &self.platform {
            BackendPlatform::Bedrock { .. } => GameEdition::Bedrock,
        };
        let state = self.state.read().await;
        let mut servers = Vec::with_capacity(state.known_servers.len());
        for weak_ref in state.known_servers.iter() {
            let server = match weak_ref.upgrade() {
                Some(server) => server,
                None => continue,
            };
            servers.push(server.into_api_model().await);
        }
        model::Backend {
            uid: self.uid.clone(),
            name: self.id.clone(),
            game_edition,
            servers,
        }
    }
}

#[async_trait::async_trait]
impl IntoApiModel for BackendServer {
    type Model = model::Server;

    async fn into_api_model(&self) -> Self::Model {
        let state = self.state.read().await;
        let health = model::ServerHealth {
            alive: state.health.alive,
            ever_alive: state.health.ever_alive,
        };
        let player_count = state.connected_players.len();
        model::Server {
            uid: self.uid.clone(),
            address: self.addr.to_string(),
            status: model::ServerStatus::Active,
            health,
            load_score: state.load_score,
            player_count,
        }
    }
}
