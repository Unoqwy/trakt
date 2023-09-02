//! Trakt web-based Dashboard.

use std::{net::SocketAddr, str::FromStr, sync::Arc};

use axum::Router;
use trakt_api::provider::TraktApi;

mod status;

pub type SharedEnv = Arc<AppEnv>;

pub struct AppEnv {
    pub api: Box<dyn TraktApi>,
}

/// Starts the Web dashboard server.
///
/// ## Arguments
///
/// * `bind` - Address to bind to
/// * `api` - API implementation to use
pub async fn start(bind: &str, api: Box<dyn TraktApi>) -> anyhow::Result<()> {
    let env = AppEnv { api };
    let env = Arc::new(env);

    let router = Router::new()
        .nest("/status", status::routes())
        .with_state(env);

    let bind_addr = SocketAddr::from_str(bind)?;
    axum::Server::bind(&bind_addr)
        .serve(router.into_make_service())
        .await?;
    Ok(())
}
