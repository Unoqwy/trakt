use std::error::Error;

use uuid::Uuid;

use crate::{
    constraint::Constraint,
    model::{Backend, Node, Server},
    BackendRefPath, HydrateOptions, ResourceRef, ServerRefPath,
};

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
    /// ## Arguments
    ///
    /// * `hydrate_opts` - Hydrate options
    ///
    /// ## Returns
    ///
    /// All nodes that are known by the implementation, each could
    /// independently be a [`Err`] if it could not be reached or another error occured.
    async fn get_nodes(&self, hydrate_opts: HydrateOptions) -> Vec<Result<Node, NodeError>>;

    /// Gets a node by UID.
    ///
    /// ## Arguments
    ///
    /// * `node_ref` - Node resource reference
    /// * `hydrate_opts` - Hydrate options
    async fn get_node(
        &self,
        node_ref: &ResourceRef,
        hydrate_opts: HydrateOptions,
    ) -> Result<Option<Node>, NodeError>;

    /// Gets a backend from a node by UID.
    ///
    /// ## Arguments
    ///
    /// * `backend_path` - Resource path to the backend
    /// * `hydrate_opts` - Hydrate options
    async fn get_backend(
        &self,
        backend_path: &BackendRefPath,
        hydrate_opts: HydrateOptions,
    ) -> Result<Option<Backend>, NodeError>;

    /// Gets a server of a backend by UID.
    ///
    /// ## Arguments
    ///
    /// * `server_path` - Resource path to the server
    /// * `hydrate_opts` - Hydrate options
    async fn get_server(
        &self,
        server_path: &ServerRefPath,
        hydrate_opts: HydrateOptions,
    ) -> Result<Option<Server>, NodeError>;

    /// Clears a server's constraints.
    ///
    /// ## Arguments
    ///
    /// * `server_path` - Resource path to the server
    async fn clear_constraints(&self, server_path: &ServerRefPath) -> Result<(), NodeError>;

    /// Clears a server's constraints.
    ///
    /// ## Arguments
    ///
    /// * `server_path` - Resource path to the server
    /// * `key` - Constraint key
    /// * `constraint` - Constraint. If [`None`] it will remove it
    async fn set_constraint(
        &self,
        server_path: &ServerRefPath,
        key: &str,
        constraint: Option<Constraint>,
    ) -> Result<(), NodeError>;
}

/// Additional API abstraction to interact with configuration.
///
/// Configuration can be handled in any form by the node (e.g. file, database).
#[async_trait::async_trait]
pub trait TraktConfigApi: Send + Sync {
    /// Reloads the configuration of all known nodes.
    async fn reload_all(&self, node_uid: &Uuid);

    /// Reloads the configuration of a node.
    ///
    /// * `node_uid` - Node to reload the configuration of
    async fn reload_node(&self, node_uid: &Uuid) -> Result<(), NodeError>;
}

impl ResourceRef {}
