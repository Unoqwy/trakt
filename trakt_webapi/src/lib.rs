//! Trakt REST API.

use std::{net::SocketAddr, str::FromStr, sync::Arc};

use axum::{routing::get, Router};
use trakt_api::{constraint, model, provider::TraktApi};

mod path;
mod resources;

pub use path::*;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

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
    #[derive(OpenApi)]
    #[openapi(
        servers(
            (url = "/v1"),
        ),
        paths(
            resources::nodes,
            resources::node,
            resources::backend,
            resources::server,
        ),
        components(
            schemas(
                UntaggedResourceRef,
                model::GameEdition,
                model::Node,
                model::Backend,
                model::Server, model::ServerStatus, model::ServerHealth,
                constraint::Constraint, constraint::ConstraintKind,
            ),
        ),
    )]
    struct ApiDoc;

    let env = AppEnv { api };
    let env = Arc::new(env);

    let v1 = Router::new()
        .route("/nodes", get(resources::nodes))
        .route("/resource/:node", get(resources::node))
        .route("/resource/:node/:backend", get(resources::backend))
        .route("/resource/:node/:backend/:server", get(resources::server));

    let router = Router::new()
        .merge(SwaggerUi::new("/v1/swagger-ui").url("/v1/openapi.json", ApiDoc::openapi()))
        .nest("/v1", v1)
        .with_state(env);

    let bind_addr = SocketAddr::from_str(bind)?;
    axum::Server::bind(&bind_addr)
        .serve(router.into_make_service())
        .await?;
    Ok(())
}
