use crate::model::Backend;

/// Read-only provider for the trakt API.
#[async_trait::async_trait]
pub trait TraktApiRead: Send + Sync {
    async fn get_backends(&self) -> Vec<Backend>;

    async fn get_backend(&self, id: &str) -> Option<Backend>;
}

/// Additional provider with write abilities to extend
/// on [`TraktApiRead`].
#[async_trait::async_trait]
pub trait TraktApiWrite {}
