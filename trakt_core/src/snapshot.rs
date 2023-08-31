use std::{fs::File, io::BufReader, path::Path};

/// A proxy server whose active connections state
/// can be saved to/loaded from a recovery snapshot.
#[async_trait::async_trait]
pub trait RecoverableProxyServer: Send + Sync {
    /// Snapshot type.
    type Snapshot;

    /// Takes a snapshot of the current proxy state.
    async fn take_snapshot(&self) -> anyhow::Result<Self::Snapshot>;

    /// Attempts to recover state from a snapshot.
    ///
    /// ## Arguments
    ///
    /// * `snapshot` - Recovery snapshot
    async fn recover_from_snapshot(&self, snapshot: Self::Snapshot);
}

/// Reads a proxy server snapshot from disk.
///
/// ## Arguments
///
/// * `path` - Snapshot file path
pub fn read_snapshot_file<P, S>(path: P) -> anyhow::Result<Option<S>>
where
    P: AsRef<Path>,
    S: serde::de::DeserializeOwned,
{
    if !path.as_ref().try_exists()? {
        return Ok(None);
    }
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let deserialized: S = serde_json::from_reader(reader)?;
    Ok(Some(deserialized))
}

/// Writes a proxy server snapshot to disk.
///
/// ## Arguments
///
/// * `path` - Snapshot file path
/// * `snapshot` - Snapshot
pub fn write_snapshot_file<P, S>(path: P, snapshot: &S) -> anyhow::Result<bool>
where
    P: AsRef<Path>,
    S: serde::ser::Serialize,
{
    let file = File::create(path)?;
    serde_json::to_writer(&file, snapshot)?;
    Ok(true)
}
