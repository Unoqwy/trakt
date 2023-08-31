//! Library to create a reverse proxy and load balancer
//! for Minecraft: Bedrock Edition servers.
//!
//! It provides both abstractions and default implementations
//! to get up and running quickly.
//!
//! The entrypoint is a [`Proxy`], that can then be configured
//! to point to one or several backends (each backend is comprised
//! of one or more servers).
//! Load balancing is achieved by using a [`LoadBalancer`]
//! on a backend.
//!
//! Note: While the focus of trakt is Minecraft: Bedrock Edition at
//! the moment, this is meant to be expandable to add Java Edition
//! support at some point.

mod backend;
pub mod bedrock;
pub mod config;
mod health;
mod load_balancer;
mod proxy;
mod scheduler;
pub mod snapshot;

pub use backend::*;
pub use health::*;
pub use load_balancer::*;
pub use proxy::*;

/// Data flow direction.
#[derive(Debug, Clone, Copy)]
pub enum Direction {
    /// Player <-> Server
    PlayerToServer,
    /// Server <-> Player
    ServerToPlayer,
}

/// Why a player disconnected from a server.
#[derive(Debug, Clone, Copy)]
pub enum DisconnectCause {
    /// Connection closed normally. Could be initiated by either
    /// the server or the client.
    Normal,
    /// Found disconnect notification from the server.
    Server,
    /// Player <-> Proxy connection timed out.
    TimeoutClient,
    /// Proxy <-> Server connection timed out.
    TimeoutServer,
    /// An unexpected error occurred.
    Error,
    /// Unknown cause.
    Unknown,
}

impl DisconnectCause {
    pub fn to_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Server => "server",
            Self::TimeoutClient => "client timeout",
            Self::TimeoutServer => "server timeout",
            Self::Error => "unexpected error",
            Self::Unknown => "unknown",
        }
    }
}
