use std::{fs, path::Path, time::SystemTime};

use serde::{Deserialize, Serialize};

use crate::config::RootConfig;

/// A snapshot of a [`crate::proxy::RaknetProxy`] state, used
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
    /// The restarting bot will use this, then once everything has
    /// recovered try to parse the requested config file.
    pub config: RootConfig,
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
    /// Proxy <-> Server bind socket address for this client.
    pub proxy_server_bind: String,
}

/// Writes a [`RaknetProxySnapshot`] into a file.
///
/// ## Arguments
///
/// * `path` - File path
pub fn write_snapshot_file<P: AsRef<Path>>(
    path: P,
    snapshot: &RaknetProxySnapshot,
) -> anyhow::Result<()> {
    let serialized = serde_json::to_string(snapshot)?;
    fs::write(path, serialized)?;
    Ok(())
}

/// Reads a [`RaknetProxySnapshot`] from a file.
///
/// ## Arguments
///
/// * `path` - File path
pub fn read_snapshot_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Option<RaknetProxySnapshot>> {
    if !path.as_ref().try_exists()? {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)?;
    let deserialized: RaknetProxySnapshot = serde_json::from_str(&contents)?;
    Ok(Some(deserialized))
}
