use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use trakt_api::{constraint::Constraint, model};
use utoipa::IntoParams;

use crate::{BackendRefParams, PathResourceRef, ServerRefParams, SharedEnv};

#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
pub struct NodeQueryParams {
    /// Whether to hydrate node backends. Defaults to `true`.
    pub hydrate_backends: Option<bool>,
    /// Whether to hydrate backend servers. Defaults to `true`.
    pub hydrate_servers: Option<bool>,
    /// Whether to hydrate server constraints. Defaults to `true`.
    pub hydrate_constraints: Option<bool>,
}

impl From<NodeQueryParams> for trakt_api::HydrateOptions {
    fn from(value: NodeQueryParams) -> Self {
        Self {
            node_backends: value.hydrate_backends.unwrap_or(true),
            backend_servers: value.hydrate_servers.unwrap_or(true),
            server_constraints: value.hydrate_constraints.unwrap_or(true),
        }
    }
}

/// List all nodes.
#[utoipa::path(
    get,
    path = "/nodes",
    params(
        NodeQueryParams,
    ),
    responses(
        (status = 200, description = "List all nodes", body = [Node])
    )
)]
pub async fn nodes(
    State(env): State<SharedEnv>,
    Query(query): Query<NodeQueryParams>,
) -> Json<Vec<model::Node>> {
    let nodes = env.api.get_nodes(query.into()).await;
    let nodes = nodes.into_iter().filter_map(|node| node.ok()).collect();
    Json(nodes)
}

/// Get a node by resource path.
#[utoipa::path(
    get,
    path = "/nodes/{node}",
    params(
        NodeQueryParams,
        ("node" = ResourceRef, Path, description = "Node resource reference"),
    ),
    responses(
        (status = 200, description = "Found node", body = Node),
        (status = NOT_FOUND, description = "Node not found"),
    )
)]
pub async fn node(
    State(env): State<SharedEnv>,
    Path(node_ref): Path<PathResourceRef>,
    Query(query): Query<NodeQueryParams>,
) -> (StatusCode, Json<Option<model::Node>>) {
    let node = env.api.get_node(&node_ref.0, query.into()).await;
    match node {
        Ok(res @ Some(_)) => (StatusCode::OK, Json(res)),
        _ => (StatusCode::NOT_FOUND, Json(None)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
pub struct BackendQueryParams {
    /// Whether to hydrate backend servers. Defaults to `true`.
    pub hydrate_servers: Option<bool>,
    /// Whether to hydrate server constraints. Defaults to `true`.
    pub hydrate_constraints: Option<bool>,
}

impl From<BackendQueryParams> for trakt_api::HydrateOptions {
    fn from(value: BackendQueryParams) -> Self {
        Self {
            node_backends: true,
            backend_servers: value.hydrate_servers.unwrap_or(true),
            server_constraints: value.hydrate_constraints.unwrap_or(true),
        }
    }
}

/// Get a backend by resource path.
#[utoipa::path(
    get,
    path = "/nodes/{node}/{backend}",
    params(
        BackendQueryParams,
        ("node" = ResourceRef, Path, description = "Node resource reference"),
        ("backend" = ResourceRef, Path, description = "Backend resource reference"),
    ),
    responses(
        (status = 200, description = "Found backend", body = Backend),
        (status = NOT_FOUND, description = "Backend not found"),
    )
)]
pub async fn backend(
    State(env): State<SharedEnv>,
    Path(path): Path<BackendRefParams>,
    Query(query): Query<BackendQueryParams>,
) -> (StatusCode, Json<Option<model::Backend>>) {
    let backend_ref = path.into();
    let backend = env.api.get_backend(&backend_ref, query.into()).await;
    match backend {
        Ok(res @ Some(_)) => (StatusCode::OK, Json(res)),
        _ => (StatusCode::NOT_FOUND, Json(None)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
pub struct ServerQueryParams {
    /// Whether to hydrate server constraints. Defaults to `true`.
    pub hydrate_constraints: Option<bool>,
}

impl From<ServerQueryParams> for trakt_api::HydrateOptions {
    fn from(value: ServerQueryParams) -> Self {
        Self {
            node_backends: true,
            backend_servers: true,
            server_constraints: value.hydrate_constraints.unwrap_or(true),
        }
    }
}

/// Get a server by resource path.
#[utoipa::path(
    get,
    path = "/nodes/{node}/{backend}/{server}",
    params(
        ServerQueryParams,
        ("node" = ResourceRef, Path, description = "Node resource reference"),
        ("backend" = ResourceRef, Path, description = "Backend resource reference"),
        ("server" = ResourceRef, Path, description = "Server resource reference"),
    ),
    responses(
        (status = 200, description = "Found server", body = Server),
        (status = NOT_FOUND, description = "Server not found"),
    )
)]
pub async fn server(
    State(env): State<SharedEnv>,
    Path(path): Path<ServerRefParams>,
    Query(query): Query<ServerQueryParams>,
) -> (StatusCode, Json<Option<model::Server>>) {
    let server_ref = path.into();
    let server = env.api.get_server(&server_ref, query.into()).await;
    match server {
        Ok(res @ Some(_)) => (StatusCode::OK, Json(res)),
        _ => (StatusCode::NOT_FOUND, Json(None)),
    }
}

/// Delete all constraints for a server.
#[utoipa::path(
    delete,
    path = "/nodes/{node}/{backend}/{server}/constraints",
    params(
        ("node" = ResourceRef, Path, description = "Node resource reference"),
        ("backend" = ResourceRef, Path, description = "Backend resource reference"),
        ("server" = ResourceRef, Path, description = "Server resource reference"),
    ),
    responses(
        (status = 200, description = "Cleared server constraints"),
        (status = NOT_FOUND, description = "Server not found"),
    )
)]
pub async fn delete_server_constraints(
    State(env): State<SharedEnv>,
    Path(path): Path<ServerRefParams>,
) -> impl IntoResponse {
    let server_ref = path.into();
    let result = env.api.clear_server_constraints(&server_ref).await;
    // FIXME: Proper errors with context
    if result.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConstraintPath {
    #[serde(flatten)]
    pub server_path: ServerRefParams,
    /// Constraint ID.
    pub constraint: String,
}

/// Create or replace a server constraint.
#[utoipa::path(
    put,
    path = "/nodes/{node}/{backend}/{server}/constraints/{constraint}",
    request_body = Constraint,
    params(
        ("node" = ResourceRef, Path, description = "Node resource reference"),
        ("backend" = ResourceRef, Path, description = "Backend resource reference"),
        ("server" = ResourceRef, Path, description = "Server resource reference"),
        ("constraint" = str, Path, description = "Constraint ID"),
    ),
    responses(
        (status = 200, description = "Cleared server constraints"),
        (status = NOT_FOUND, description = "Server not found"),
    )
)]
pub async fn put_server_constraint(
    State(env): State<SharedEnv>,
    Path(path): Path<ServerConstraintPath>,
    Json(constraint): Json<Constraint>,
) -> impl IntoResponse {
    let server_ref = path.server_path.into();
    let result = env
        .api
        .set_server_constraint(&server_ref, &path.constraint, Some(constraint))
        .await;
    // FIXME: Proper errors with context
    if result.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}
