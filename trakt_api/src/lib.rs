//! API models and providers that enable
//! integration with Trakt proxies.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod constraint;
pub mod model;
pub mod provider;

/// A reference to an API resource (node, backend, server).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceRef {
    /// Reference by API UID.
    Uid(Uuid),
    /// Reference by name/slug.
    ///
    /// If several of the same resources use the same name in the same scope
    /// (e.g. two servers with the same name in the same backend),
    /// this may cause inconsistent behavior.
    /// Prefer using UUIDs unless you certain everything is setup properly.
    Name(String),
}

/// A reference path to a backend.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BackendRefPath {
    pub node: ResourceRef,
    pub backend: ResourceRef,
}

/// A reference path to a server.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServerRefPath {
    pub node: ResourceRef,
    pub backend: ResourceRef,
    pub server: ResourceRef,
}

/// Hydrate options for API requests.
///
/// This enables fetching only the wanted data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydrateOptions {
    /// Whether to hydrate [`model::Node::backends`].
    pub node_backends: bool,
    /// Whether to hydrate [`model::Backend::servers`].
    pub backend_servers: bool,
    /// Whether to hydrate [`model::Server::constraints`].
    pub server_constraints: bool,
}

impl ResourceRef {
    pub const fn by_uid(uid: Uuid) -> Self {
        Self::Uid(uid)
    }

    pub const fn by_name(name: String) -> Self {
        Self::Name(name)
    }
}

impl HydrateOptions {
    /// Returns [`HydrateOptions`] with everything disabled.
    pub const fn none() -> Self {
        Self {
            node_backends: false,
            backend_servers: false,
            server_constraints: false,
        }
    }

    /// Returns [`HydrateOptions`] with everything enabled.
    pub const fn all() -> Self {
        Self {
            node_backends: true,
            backend_servers: true,
            server_constraints: true,
        }
    }
}
