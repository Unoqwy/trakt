use std::collections::HashMap;
use std::fmt::Display;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::constraint::Constraint;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa_schemas", derive(utoipa::ToSchema))]
pub enum GameEdition {
    /// Bedrock Edition.
    Bedrock,
}

/// A node is an instance of a proxy, that could be running anywhere.
/// For example, running the default binary will start a node.
/// Several nodes can run on the same machine, just like nodes
/// can run across different machines.
/// By default, each node exposes its own HTTP API and a master controller
/// can be used to merge them behind a unique HTTP API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa_schemas", derive(utoipa::ToSchema))]
pub struct Node {
    /// API unique ID.
    pub uid: Uuid,
    /// Node name.
    pub name: String,
    /// Backends. Null if not hydrated.
    pub backends: Option<Vec<Backend>>,
}

/// A backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa_schemas", derive(utoipa::ToSchema))]
pub struct Backend {
    /// API unique ID.
    pub uid: Uuid,
    /// Backend name.
    pub name: String,
    /// Game edition.
    pub game_edition: GameEdition,
    /// Servers. Null if not hydrated.
    pub servers: Option<Vec<Server>>,
}

/// A backend server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa_schemas", derive(utoipa::ToSchema))]
pub struct Server {
    /// API unique ID.
    pub uid: Uuid,
    /// Remote server address.
    pub address: String,
    /// Whether the remote server uses proxy protocol.
    pub proxy_protocol: bool,
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
    /// Constraints. Null if not hydrated.
    pub constraints: Option<HashMap<String, Constraint>>,
}

/// Status of a server regarding its joinability.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa_schemas", derive(utoipa::ToSchema))]
pub enum ServerStatus {
    /// The server is active.
    Active,
    /// The server was removed but still has players online.
    Stale,
}

/// Health status of a server.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa_schemas", derive(utoipa::ToSchema))]
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
