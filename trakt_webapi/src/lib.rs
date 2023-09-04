//! Trakt REST API.

use std::{net::SocketAddr, str::FromStr, sync::Arc};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use trakt_api::{model, provider::TraktApi, HydrateOptions, ResourceRef};

use uuid::Uuid;

pub type SharedEnv = Arc<AppEnv>;

pub struct AppEnv {
    pub api: Box<dyn TraktApi>,
}

/// Starts the REST API server.
///
/// ## Arguments
///
/// * `bind` - Address to bind to
/// * `api` - API implementation to use
pub async fn start(bind: &str, api: Box<dyn TraktApi>) -> anyhow::Result<()> {
    let env = AppEnv { api };
    let env = Arc::new(env);

    let router = Router::new()
        .route("/nodes", get(nodes))
        .route("/node/:node", get(node))
        .with_state(env);

    let bind_addr = SocketAddr::from_str(bind)?;
    axum::Server::bind(&bind_addr)
        .serve(router.into_make_service())
        .await?;
    Ok(())
}

async fn nodes(State(env): State<SharedEnv>) -> Json<Vec<model::Node>> {
    let nodes = env.api.get_nodes(HydrateOptions::all()).await;
    let nodes = nodes.into_iter().filter_map(|node| node.ok()).collect();
    Json(nodes)
}

async fn node(
    Path(node_id): Path<Uuid>,
    State(env): State<SharedEnv>,
) -> (StatusCode, Json<Option<model::Node>>) {
    let node = env
        .api
        .get_node(&ResourceRef::by_uid(node_id), HydrateOptions::all())
        .await;
    match node {
        Ok(node @ Some(_)) => (StatusCode::OK, Json(node)),
        _ => (StatusCode::NOT_FOUND, Json(None)),
    }
}
