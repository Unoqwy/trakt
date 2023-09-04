//! Trakt Reverse proxy.

use std::{path::PathBuf, sync::Arc};

use anyhow::Context;
use trakt_api::ResourceRef;

use crate::{
    config::RuntimeConfigProvider,
    scheduler::Scheduler,
    snapshot::{self, RecoverableProxyServer},
    Backend,
};

/// [`Proxy`] is a wrapper around a [`ProxyServer`],
/// that also deals with common parts across server implementations.
pub struct Proxy<S: ProxyServer> {
    /// Proxy server.
    pub server: Arc<S>,

    /// Runtime config provider.
    pub config_provider: Arc<RuntimeConfigProvider>,
    /// Scheduler.
    pub scheduler: Scheduler<S>,

    /// Recovery snapshot file.
    pub recovery_snapshot_file: Option<PathBuf>,
}

/// A proxy server listen to/manage connections,
/// and forward traffic to backend servers.
#[async_trait::async_trait]
pub trait ProxyServer: Send + Sync {
    /// Runs the proxy server.
    ///
    /// If stopped graciously it will return [`Ok`],
    /// otherwise it wil return an error.
    async fn run(self: Arc<Self>) -> anyhow::Result<()>;

    /// Gets all the backends known/managed by the proxy server.
    async fn get_backends(&self) -> Vec<Arc<Backend>>;

    /// Gets a backend by resource reference.
    async fn get_backend(&self, backend_ref: &ResourceRef) -> Option<Arc<Backend>>;
}

impl<S> Proxy<S>
where
    S: ProxyServer + 'static,
{
    pub fn new(
        server: Arc<S>,
        config_provider: Arc<RuntimeConfigProvider>,
        recovery_snapshot_file: Option<PathBuf>,
    ) -> Self {
        let scheduler = Scheduler::new(config_provider.clone(), server.clone());
        Self {
            server,
            config_provider,
            scheduler,
            recovery_snapshot_file,
        }
    }

    /// Runs the underlying proxy server and takes care of starting/stopping the scheduler.
    pub async fn run(&self) -> anyhow::Result<()> {
        self.scheduler.start();
        let result = ProxyServer::run(self.server.clone()).await;
        self.scheduler.stop(true).await;
        result
    }

    /// Propagates changes from the config provider.
    pub async fn reload_config(&self) {
        if self.scheduler.is_running() {
            self.scheduler.restart().await;
        }
    }
}

impl<S, Sp> Proxy<S>
where
    S: ProxyServer + RecoverableProxyServer<Snapshot = Sp>,
    Sp: serde::ser::Serialize + serde::de::DeserializeOwned,
{
    /// Takes a snapshot of the proxy server and try to write it to disk.
    pub async fn take_and_write_snapshot(&self) -> anyhow::Result<bool> {
        let path = match &self.recovery_snapshot_file {
            Some(path) => path,
            None => return Ok(false),
        };
        let snapshot = self
            .server
            .take_snapshot()
            .await
            .context("Could not take proxy server snapshot")?;
        snapshot::write_snapshot_file(path, &snapshot)
            .context("Could not write proxy server snapshot to disk")
    }
}
