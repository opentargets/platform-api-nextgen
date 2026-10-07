//! HTTP server: router assembly and startup.

use std::net::SocketAddr;

use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    Router,
    extract::State,
    middleware::from_fn,
    response::{Html, IntoResponse},
    routing::{any, get},
};
use axum_server::tls_rustls::RustlsConfig;
use reqwest::{Method, header};
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
};

use crate::{AppState, plugin::handler_rest::handle, server::cache::post_cache};

// Some static assets served directly by the HTTP server.
const GRAPHIQL: &str = include_str!("assets/graphiql.html");
const FAVICON: &[u8] = include_bytes!("assets/favicon.png");
async fn graphiql() -> Html<&'static str> { Html(GRAPHIQL) }
async fn favicon() -> impl IntoResponse { ([(header::CONTENT_TYPE, "image/png")], FAVICON) }

// The GraphQL part of the API.
async fn graphql(State(state): State<AppState>, req: GraphQLRequest) -> GraphQLResponse {
    state.api.execute(req.into_inner()).await.into()
}

/// Returns the HTTP router for the API.
pub fn router(state: AppState) -> Router {
    let release = format!("/{}", state.config.data_release_main());
    let api: Router<AppState> = Router::new().route(
        "/graphql",
        get(graphiql).post(graphql).layer(from_fn(post_cache)),
    );

    Router::new()
        .route("/favicon.ico", get(favicon))
        .route("/plugin/{*path}", any(handle))
        .nest(&release, api.clone()) // e.g. `/2606` for data release `26.06.1`
        .nest("/latest", api.clone()) // Immutable route to the latest data release
        .nest("/api/v4", api) // Keeps backwards-compatible route
        .layer(CompressionLayer::new().br(true))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers(Any),
        )
        .with_state(state)
}

/// Starts the HTTP server and listens for incoming requests.
///
/// # Panics
/// Panics if the server fails to bind to the specified address or if there is a server error during
/// execution.
pub async fn serve(state: AppState) {
    let cfg = &state.config;
    let addr: SocketAddr = cfg.bind_address.parse().expect("invalid bind address");

    match (&cfg.cert_file, &cfg.key_file) {
        (Some(cert), Some(key)) => {
            let tls = RustlsConfig::from_pem_file(cert, key)
                .await
                .unwrap_or_else(|e| panic!("failed to load tls cert or key: {e}"));
            tracing::info!("listening on https://{addr}");
            axum_server::bind_rustls(addr, tls)
                .serve(router(state).into_make_service())
                .await
                .expect("server error");
        }
        (None, None) => {
            tracing::info!("listening on http://{addr}");
            axum_server::bind(addr)
                .serve(router(state).into_make_service())
                .await
                .expect("server error");
        }
        _ => panic!("cert_file and key_file must both be set or both unset"),
    }
}
