use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::config::RuntimeConfig;

/// A snapshot of a [`crate::bedrock::RaknetProxyServer`], used
/// to recover UDP connections after a restart (if it only takes a few seconds).
///
/// It contains only the necessary information to recover clients,
/// it does not mean to be a 1:1 representation of the proxy state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaknetProxySnapshot {
    /// Time at which the snapshot was taken.
    ///
    /// If too much time has elapsed (i.e. more than a few seconds),
    /// the snapshot won't even try to load as clients have most likley
    /// already determined the server was dead and disconnected.
    pub taken_at: SystemTime,
    /// Configuration in use at the time of the snapshot.
    ///
    /// The restarting proxy will use this first, then once everything
    /// has recovered, it will try reload the configuration.
    pub config: RuntimeConfig,
    /// Player <-> Proxy bind socket address.
    pub player_proxy_bind: String,
    /// Connected clients.
    ///
    /// Active clients that are not connected are OK to drop.
    pub clients: Vec<RaknetClientSnapshot>,
}

/// Snapshot that can be used to recover an active client connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaknetClientSnapshot {
    /// Remote client socket address.
    pub addr: String,
    /// Socket address of the backend server.
    pub server_addr: String,
    /// Whether proxy protocol is enabled for the server.
    #[serde(default)] // since 0.2.0
    pub server_proxy_protocol: bool,
    /// Proxy <-> Server bind socket address for this client.
    pub proxy_server_bind: String,
}

impl RaknetProxySnapshot {
    pub fn has_expired(&self) -> bool {
        self.taken_at
            .elapsed()
            .map(|elapsed| elapsed >= Duration::from_secs(10))
            .unwrap_or(true)
    }
}
