use std::error::Error;

use uuid::Uuid;

use crate::model::{Backend, Node, Server};

/// Nodes may be remote and data exchange with the API can fail.
/// In such cases, the error should be wrapped in [`NodeError`].
///
/// A UID and name are expected, as nodes would not be exposed
/// to the API if either piece of information is unknown.
///
/// For example, on a system with automatic discovery,
/// [`TraktApi::get_node`] would return [`None`] instead of an error
/// while the controller has not been made aware of the node.
pub struct NodeError {
    /// Node UID.
    pub node_uid: Uuid,
    /// Node name.
    pub node_name: String,
    /// Error.
    pub inner: Box<dyn Error>,
}

/// API abstraction.
///
/// A node is a instance of a proxy, that can run anywhere.
/// For example, several nodes can run on the same machine,
/// and even more run on different machines.
#[async_trait::async_trait]
pub trait TraktApi: Send + Sync {
    /// Gets all nodes.
    ///
    /// * `hydrate` - Whether to return all backends
    async fn get_nodes(&self, hydrate: bool) -> Vec<Result<Node, NodeError>>;

    /// Gets a node by UID.
    ///
    /// ## Arguments
    ///
    /// * `node_uid` - Node API UID
    async fn get_node(&self, node_uid: &Uuid) -> Result<Option<Node>, NodeError>;

    /// Gets a backend from a node by UID.
    ///
    /// ## Arguments
    ///
    /// * `node_uid` - Node API UID
    /// * `backend_uid` - Backend API UID
    /// * `hydrate` - Whether to return all servers
    async fn get_backend(
        &self,
        node_uid: &Uuid,
        backend_uid: &Uuid,
        hydrate: bool,
    ) -> Result<Option<Backend>, NodeError>;

    /// Gets a server of a backend by UID.
    ///
    /// ## Arguments
    ///
    /// * `node_uid` - Node API UID
    /// * `backend_uid` - Backend API UID
    /// * `server_uid` - Server API UID
    async fn get_server(
        &self,
        node_uid: &Uuid,
        backend_uid: &Uuid,
        server_uid: &Uuid,
    ) -> Result<Option<Server>, NodeError>;
}

/// Additional API to interact with a node's configuration.
#[async_trait::async_trait]
pub trait TraktConfigApi: Send + Sync {
    /// Reloads the configuration.
    async fn reload(&self);
}
