//! HTTP server: router assembly and startup.

use axum::{
    Extension, Router,
    middleware::from_fn,
    response::{Html, IntoResponse},
    routing::get,
};
use reqwest::header;
use tokio::net::TcpListener;
use tower_http::compression::CompressionLayer;

use crate::{
    AppState,
    server::{
        cache::post_cache,
        graphql::{self, ApiSchema},
    },
};

const GRAPHIQL: &str = include_str!("assets/graphiql.html");
const FAVICON: &[u8] = include_bytes!("assets/favicon.png");

async fn graphiql() -> Html<&'static str> { Html(GRAPHIQL) }
async fn favicon() -> impl IntoResponse { ([(header::CONTENT_TYPE, "image/png")], FAVICON) }

pub fn router(state: AppState, schema: ApiSchema) -> Router {
    let release = format!("/{}", state.config.data_release_main());

    let api: Router<AppState> = Router::new().route(
        "/graphql",
        get(graphiql)
            .post(graphql::handler)
            .layer(from_fn(post_cache)),
    );

    Router::new()
        .route("/favicon.ico", get(favicon))
        .nest(&release, api.clone()) // e.g. `/2606` for data release `26.06.1`
        .nest("/latest", api.clone()) // Immutable route to the latest data release
        .nest("/api/v4", api) // Keeps backwards-compatible route
        .layer(Extension(schema))
        .layer(Extension(state.opensearch.clone()))
        .layer(Extension(state.clickhouse.clone()))
        .layer(CompressionLayer::new().br(true))
        .with_state(state)
}

/// Starts the HTTP server and listens for incoming requests.
///
/// # Panics
/// Panics if the server fails to bind to the specified address or if there is a server error during
/// execution.
pub async fn serve(state: AppState, schema: ApiSchema) {
    let addr = &state.config.bind_address;
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));

    tracing::info!("listening on {addr}");
    axum::serve(listener, router(state, schema))
        .await
        .expect("server error");
}
