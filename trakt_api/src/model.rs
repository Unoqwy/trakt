use std::fmt::Display;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameEdition {
    /// Bedrock Edition.
    Bedrock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proxy {
    /// API unique ID.
    pub uid: Uuid,
    /// Proxy name.
    pub name: String,
    /// Backends.
    pub backends: Vec<Backend>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backend {
    /// API unique ID.
    pub uid: Uuid,
    /// Backend name.
    pub name: String,
    /// Game edition.
    pub game_edition: GameEdition,
    /// Servers.
    pub servers: Vec<Server>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    /// API unique ID.
    pub uid: Uuid,
    /// Remote server address.
    pub address: String,
    /// Server status.
    pub status: ServerStatus,
    /// Server health.
    pub health: ServerHealth,
    /// Load score.
    pub load_score: usize,
    /// Number of online players.
    ///
    /// Only accounts for players connected through the proxy,
    /// more may be online if connected from other sources.
    pub player_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    /// The server is active.
    Active,
    /// The server was removed but still has players online.
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServerHealth {
    /// Whether the server is alive, and joinable.
    pub alive: bool,
    /// Whether the server was ever alive since the proxy start.
    pub ever_alive: bool,
}

impl Display for GameEdition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bedrock => write!(f, "Bedrock"),
        }
    }
}

impl Display for ServerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "Active"),
            Self::Stale => write!(f, "Stale"),
        }
    }
}
