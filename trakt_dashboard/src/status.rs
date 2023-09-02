use askama::Template;
use axum::{extract::State, routing::get, Router};
use trakt_api::{model, provider::NodeError};

use crate::SharedEnv;

#[derive(Template)]
#[template(path = "status/index.html")]
struct IndexTemplate {
    nodes: Vec<Result<model::Node, NodeError>>,
}

#[derive(Template)]
#[template(path = "status/_inner.html")]
struct HxInnerTemplate {
    nodes: Vec<Result<model::Node, NodeError>>,
}

mod filters {
    pub fn or_emptyvec<T>(opt: &Option<Vec<T>>) -> ::askama::Result<Vec<&T>> {
        match opt {
            Some(vec) => Ok(vec.iter().collect()),
            None => Ok(vec![]),
        }
    }
}

pub fn routes() -> Router<SharedEnv> {
    Router::new()
        .route("/", get(index))
        .route("/_hx_refresh", get(hx_refresh))
}

async fn index(State(env): State<SharedEnv>) -> IndexTemplate {
    let nodes = env.api.get_nodes(true).await;
    IndexTemplate { nodes }
}

async fn hx_refresh(State(env): State<SharedEnv>) -> HxInnerTemplate {
    let nodes = env.api.get_nodes(true).await;
    HxInnerTemplate { nodes }
}
