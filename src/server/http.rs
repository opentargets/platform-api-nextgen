//! HTTP server: router assembly and startup.

use axum::{
    Extension, Router,
    body::{Body, Bytes},
    extract::{Path, RawQuery, State},
    http::HeaderMap,
    middleware::from_fn,
    response::{Html, IntoResponse, Response},
    routing::{any, get},
};
use reqwest::{
    Method, StatusCode, header,
    header::{
        CONNECTION, HeaderName, PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE, TRAILER,
        TRANSFER_ENCODING, UPGRADE,
    },
};
use tokio::net::TcpListener;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
};

use crate::{
    AppState,
    server::{
        cache::post_cache,
        graphql::{self, ApiSchema},
    },
};

const GRAPHIQL: &str = include_str!("assets/graphiql.html");
const FAVICON: &[u8] = include_bytes!("assets/favicon.png");
const HOP_BY_HOP: [HeaderName; 8] = [
    CONNECTION,
    HeaderName::from_static("keep-alive"),
    PROXY_AUTHENTICATE,
    PROXY_AUTHORIZATION,
    TE,
    TRAILER,
    TRANSFER_ENCODING,
    UPGRADE,
];

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

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    Router::new()
        .route("/favicon.ico", get(favicon))
        .route("/plugin/{*path}", any(plugin_handler))
        .nest(&release, api.clone()) // e.g. `/2606` for data release `26.06.1`
        .nest("/latest", api.clone()) // Immutable route to the latest data release
        .nest("/api/v4", api) // Keeps backwards-compatible route
        .layer(Extension(schema))
        .layer(Extension(state.opensearch.clone()))
        .layer(Extension(state.clickhouse.clone()))
        .layer(CompressionLayer::new().br(true))
        .layer(cors)
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

async fn plugin_handler(
    State(app): State<AppState>,
    Path(path): Path<String>,
    method: Method,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Result<Response, StatusCode> {
    // Divide the path so we know the plugin name.
    let (name, rest) = path.split_once('/').unwrap_or((&path, ""));
    let plugin = app.plugin_registry.get(name).ok_or(StatusCode::NOT_FOUND)?;

    // Rebuild with the plugin URL and path/query.
    let mut url = plugin.url();
    url.set_path(&format!("{}/{}", url.path().trim_end_matches('/'), rest));
    url.set_query(query.as_deref());

    // Keep headers except HOP_BY_HOP.
    let mut req = app.http.request(method, url).body(body);
    if let Some(ct) = headers.get(axum::http::header::CONTENT_TYPE) {
        req = req.header(reqwest::header::CONTENT_TYPE, ct);
    }

    // Perform the request to the plugin.
    let upstream = req.send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let status = upstream.status();

    // Build the response from the upstream response.
    let mut response = Response::builder().status(status);
    let headers = response.headers_mut().unwrap();
    for (name, value) in upstream.headers() {
        if !HOP_BY_HOP.contains(name) {
            headers.insert(name, value.clone());
        }
    }
    let bytes = upstream
        .bytes()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    response
        .body(Body::from(bytes))
        .map_err(|_| StatusCode::BAD_GATEWAY)
}
