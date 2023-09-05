//! Trakt HTTP API.

use std::{net::SocketAddr, str::FromStr, sync::Arc};

use axum::{
    routing::{delete, get, put},
    Router,
};
use trakt_api::{constraint, model, provider::TraktApi, ResourceRef};
use utoipa::OpenApi;
use utoipa_rapidoc::RapiDoc;
use utoipa_swagger_ui::SwaggerUi;

mod path;
mod resources;

pub use path::*;

pub type SharedEnv = Arc<AppEnv>;

pub struct AppEnv {
    pub api: Box<dyn TraktApi>,
}

/// Starts the HTTP API server.
///
/// ## Arguments
///
/// * `bind` - Address to bind to
/// * `api` - API implementation to use
pub async fn start(bind: &str, api: Box<dyn TraktApi>) -> anyhow::Result<()> {
    #[derive(OpenApi)]
    #[openapi(
        info(
            title = "Trakt API",
            description = include_str!("../description.md"),
        ),
        servers(
            (url = "/v0", description = "Beta version, subject to breaking changes"),
        ),
        paths(
            resources::nodes,
            resources::node,
            resources::backend,
            resources::server,
            resources::delete_server_constraints,
            resources::put_server_constraint,
        ),
        components(
            schemas(
                ResourceRef,
                model::GameEdition,
                model::Node,
                model::Backend,
                model::Server, model::ServerStatus, model::ServerHealth,
                constraint::Constraint, constraint::ConstraintKind,
            ),
        ),
        tags(
            (name = "resources", description = "View and control active resources (nodes, backends, servers)")
        ),
    )]
    struct ApiDoc;

    let env = AppEnv { api };
    let env = Arc::new(env);

    let v0 = Router::new()
        .route("/nodes", get(resources::nodes))
        .route("/nodes/:node", get(resources::node))
        .route("/nodes/:node/:backend", get(resources::backend))
        .route("/nodes/:node/:backend/:server", get(resources::server))
        .route(
            "/nodes/:node/:backend/:server/constraints",
            delete(resources::delete_server_constraints),
        )
        .route(
            "/nodes/:node/:backend/:server/constraints/:constraint",
            put(resources::put_server_constraint),
        );

    let router = Router::new()
        .merge(SwaggerUi::new("/v0/swagger-ui").url("/v0/openapi.json", ApiDoc::openapi()))
        .merge(RapiDoc::new("/v0/openapi.json").path("/v0/rapidoc"))
        .nest("/v0", v0)
        .with_state(env);

    let bind_addr = SocketAddr::from_str(bind)?;
    axum::Server::bind(&bind_addr)
        .serve(router.into_make_service())
        .await?;
    Ok(())
}
