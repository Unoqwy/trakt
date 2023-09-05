use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use trakt_api::{model, HydrateOptions};

use crate::{BackendRefParams, PathResourceRef, ServerRefParams, SharedEnv};

#[utoipa::path(
    get,
    path = "/nodes",
    responses(
        (status = 200, description = "List all nodes", body = [Node])
    )
)]
pub async fn nodes(State(env): State<SharedEnv>) -> Json<Vec<model::Node>> {
    let nodes = env.api.get_nodes(HydrateOptions::all()).await;
    let nodes = nodes.into_iter().filter_map(|node| node.ok()).collect();
    Json(nodes)
}

#[utoipa::path(
    get,
    path = "/resource/{node}",
    params(
        ("node" = ResourceRef, Path, description = "Node resource reference"),
    ),
    responses(
        (status = 200, description = "Found node", body = Node),
        (status = NOT_FOUND, description = "Node not found"),
    )
)]
pub async fn node(
    Path(node_ref): Path<PathResourceRef>,
    State(env): State<SharedEnv>,
) -> (StatusCode, Json<Option<model::Node>>) {
    let node = env.api.get_node(&node_ref.0, HydrateOptions::all()).await;
    match node {
        Ok(res @ Some(_)) => (StatusCode::OK, Json(res)),
        _ => (StatusCode::NOT_FOUND, Json(None)),
    }
}

#[utoipa::path(
    get,
    path = "/resource/{node}/{backend}",
    params(
        ("node" = ResourceRef, Path, description = "Node resource reference"),
        ("backend" = ResourceRef, Path, description = "Backend resource reference"),
    ),
    responses(
        (status = 200, description = "Found backend", body = Node),
        (status = NOT_FOUND, description = "Backend not found"),
    )
)]
pub async fn backend(
    Path(path): Path<BackendRefParams>,
    State(env): State<SharedEnv>,
) -> (StatusCode, Json<Option<model::Backend>>) {
    let backend_ref = path.into();
    let backend = env
        .api
        .get_backend(&backend_ref, HydrateOptions::all())
        .await;
    match backend {
        Ok(res @ Some(_)) => (StatusCode::OK, Json(res)),
        _ => (StatusCode::NOT_FOUND, Json(None)),
    }
}

#[utoipa::path(
    get,
    path = "/resource/{node}/{backend}/{server}",
    params(
        ("node" = ResourceRef, Path, description = "Node resource reference"),
        ("backend" = ResourceRef, Path, description = "Backend resource reference"),
        ("server" = ResourceRef, Path, description = "Server resource reference"),
    ),
    responses(
        (status = 200, description = "Found server", body = Node),
        (status = NOT_FOUND, description = "Server not found"),
    )
)]
pub async fn server(
    Path(path): Path<ServerRefParams>,
    State(env): State<SharedEnv>,
) -> (StatusCode, Json<Option<model::Server>>) {
    let server_ref = path.into();
    let backend = env.api.get_server(&server_ref, HydrateOptions::all()).await;
    match backend {
        Ok(res @ Some(_)) => (StatusCode::OK, Json(res)),
        _ => (StatusCode::NOT_FOUND, Json(None)),
    }
}
