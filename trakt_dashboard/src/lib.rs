//! Trakt Web dashboard.

use std::{net::SocketAddr, str::FromStr, sync::Arc};

use askama::Template;
use axum::{extract::State, routing, Router};
use tower_livereload::LiveReloadLayer;
use trakt_api::{model, provider::TraktApiRead};

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    backends: Vec<model::Backend>,
}

#[derive(Template)]
#[template(path = "status.html")]
struct StatusPartialTemplate {
    backends: Vec<model::Backend>,
}

struct AppState {
    read_api: Box<dyn TraktApiRead>,
}

pub async fn start(read_api: Box<dyn TraktApiRead>) -> anyhow::Result<()> {
    let state = AppState { read_api };
    let state = Arc::new(state);

    let app = Router::new()
        .route("/status", routing::get(status))
        .route("/status/_partial", routing::get(status_partial))
        .with_state(state);
        //.layer(LiveReloadLayer::new());

    let bind_addr = SocketAddr::from_str("0.0.0.0:8081")?;
    axum::Server::bind(&bind_addr)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

async fn status(State(state): State<Arc<AppState>>) -> IndexTemplate {
    let backends = state.read_api.get_backends().await;
    IndexTemplate { backends }
}

async fn status_partial(State(state): State<Arc<AppState>>) -> StatusPartialTemplate {
    let backends = state.read_api.get_backends().await;
    StatusPartialTemplate { backends }
}
