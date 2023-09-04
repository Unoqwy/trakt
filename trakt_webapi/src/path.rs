use serde::{Deserialize, Serialize};
use trakt_api::ResourceRef;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged, remote = "ResourceRef")]
#[derive(ToSchema)]
#[schema(as = ResourceRef)]
pub enum UntaggedResourceRef {
    Uid(Uuid),
    Name(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathResourceRef(#[serde(with = "UntaggedResourceRef")] pub ResourceRef);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendRefParams {
    pub node: PathResourceRef,
    pub backend: PathResourceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerRefParams {
    pub node: PathResourceRef,
    pub backend: PathResourceRef,
    pub server: PathResourceRef,
}

impl From<BackendRefParams> for trakt_api::BackendRefPath {
    fn from(value: BackendRefParams) -> Self {
        Self {
            node: value.node.0,
            backend: value.backend.0,
        }
    }
}

impl From<ServerRefParams> for trakt_api::ServerRefPath {
    fn from(value: ServerRefParams) -> Self {
        Self {
            node: value.node.0,
            backend: value.backend.0,
            server: value.server.0,
        }
    }
}
