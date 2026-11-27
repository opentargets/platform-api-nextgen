//! HTTP server: router assembly and startup.

use std::net::SocketAddr;

use async_graphql::ParseRequestError;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    middleware::{Next, from_fn},
    response::{Html, IntoResponse, Response},
    routing::{any, get},
};
use axum_prometheus::PrometheusMetricLayerBuilder;
use axum_server::tls_rustls::RustlsConfig;
use reqwest::{Method, header};
use serde_json::json;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
};

use crate::{
    AppState,
    plugin::handler_rest::handle,
    server::{cache::post_cache, log_utils::client_ip},
};
// Some static assets served directly by the HTTP server.
const GRAPHIQL: &str = include_str!("assets/graphiql.html");
const FAVICON: &[u8] = include_bytes!("assets/favicon.png");
async fn graphiql() -> Html<&'static str> { Html(GRAPHIQL) }
async fn favicon() -> impl IntoResponse { ([(header::CONTENT_TYPE, "image/png")], FAVICON) }
async fn health() -> &'static str { "OK" }

/// The full GraphQL schema as SDL text.
async fn schema(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        state.api.sdl(),
    )
}

/// Rejection for GraphQL requests that cannot be parsed, e.g. malformed JSON or a batch.
///
/// The default `async-graphql-axum` rejection puts the `Debug` text of the error in the body,
/// which is not JSON. This one answers with a GraphQL error response instead, so clients get
/// the same JSON shape as for any other error.
struct RequestRejection(ParseRequestError);

impl From<ParseRequestError> for RequestRejection {
    fn from(err: ParseRequestError) -> Self { Self(err) }
}

impl IntoResponse for RequestRejection {
    fn into_response(self) -> Response {
        let (status, message) = match self.0 {
            ParseRequestError::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "Payload too large".to_string(),
            ),
            ParseRequestError::UnsupportedBatch => (
                StatusCode::BAD_REQUEST,
                "Batch requests are not supported. Send one query per request.".to_string(),
            ),
            err => (StatusCode::BAD_REQUEST, err.to_string()),
        };
        // A request error has no `data` field, only `errors` (GraphQL over HTTP).
        (status, Json(json!({ "errors": [{ "message": message }] }))).into_response()
    }
}

// The GraphQL part of the API.
async fn graphql(
    State(state): State<AppState>,
    req: GraphQLRequest<RequestRejection>,
) -> GraphQLResponse {
    state.api.execute(req.into_inner()).await.into()
}

/// Logs the client IP address of requests reaching the GraphQL endpoint.
async fn log_client_ip(req: axum::extract::Request, next: Next) -> axum::response::Response {
    let ip = client_ip(req.headers());
    tracing::info!(client_ip = %ip, "request received from ip {}", ip);
    next.run(req).await
}

/// Returns the HTTP router for the API.
pub fn router(state: AppState) -> (Router, Router) {
    let release = format!("/{}", state.config.data_release_main());
    let api: Router<AppState> = Router::new()
        .route(
            "/graphql",
            get(graphiql)
                .post(graphql)
                .layer(from_fn(post_cache))
                .layer(from_fn(log_client_ip)),
        )
        .route("/graphql/schema", get(schema));

    let (_, metric_handle) = axum_prometheus::PrometheusMetricLayer::pair();
    let prom_layer = PrometheusMetricLayerBuilder::new()
        .with_prefix("platform_api")
        .build();
    let metrics_router =
        Router::new().route("/metrics", get(|| async move { metric_handle.render() }));

    let api_router = Router::new()
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
        .layer(prom_layer)
        .route("/health", get(health))
        .with_state(state);
    (api_router, metrics_router)
}

/// Starts the HTTP server and listens for incoming requests.
///
/// # Panics
/// Panics if the server fails to bind to the specified address or if there is a server error during
/// execution.
pub async fn serve(state: AppState) {
    let cfg = &state.config;
    let addr: SocketAddr = cfg.bind_address.parse().expect("invalid bind address");

    let metrics_addr: SocketAddr = cfg
        .metrics_bind_address
        .parse()
        .expect("invalid metrics bind address");

    match (&cfg.cert_file, &cfg.key_file) {
        (Some(cert), Some(key)) => {
            let tls = RustlsConfig::from_pem_file(cert, key)
                .await
                .unwrap_or_else(|e| panic!("failed to load tls cert or key: {e}"));
            tracing::info!("listening on https://{addr}");
            tracing::info!("Metrics listening on {metrics_addr}");

            let (api_router, metrics_router) = router(state);

            let _ = tokio::try_join!(
                axum_server::bind_rustls(addr, tls.clone()).serve(api_router.into_make_service()),
                axum_server::bind_rustls(metrics_addr, tls)
                    .serve(metrics_router.into_make_service()),
            );
        }
        (None, None) => {
            tracing::info!("listening on http://{addr}");
            tracing::info!("Metrics listening on {metrics_addr}");

            let (api_router, metrics_router) = router(state);

            let _ = tokio::try_join!(
                axum_server::bind(addr).serve(api_router.into_make_service()),
                axum_server::bind(metrics_addr).serve(metrics_router.into_make_service()),
            );
        }
        _ => panic!("cert_file and key_file must both be set or both unset"),
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        extract::FromRequest,
        http::Request,
    };
    use serde_json::Value;

    use super::*;

    /// Parses `body` as a GraphQL request and returns the rejection response, if any.
    async fn reject(body: &'static str) -> Option<(StatusCode, Option<String>, Value)> {
        let req = Request::post("/graphql")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap();
        let rejection = GraphQLRequest::<RequestRejection>::from_request(req, &())
            .await
            .err()?;
        let resp = rejection.into_response();
        let status = resp.status();
        let content_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap().to_string());
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        Some((
            status,
            content_type,
            serde_json::from_slice(&bytes).unwrap(),
        ))
    }

    #[tokio::test]
    async fn malformed_json_is_a_json_graphql_error() {
        let (status, content_type, body) = reject(r#"{"query": "#).await.unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(content_type.as_deref(), Some("application/json"));
        assert_eq!(
            body,
            json!({ "errors": [{
                "message": "Invalid request: EOF while parsing a value at line 1 column 10"
            }] })
        );
    }

    #[tokio::test]
    async fn non_json_body_is_a_json_graphql_error() {
        let (status, _, body) = reject("hello").await.unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body,
            json!({ "errors": [{
                "message": "Invalid request: expected value at line 1 column 1"
            }] })
        );
    }

    #[tokio::test]
    async fn batch_request_says_batches_are_not_supported() {
        let (status, _, body) =
            reject(r#"[{"query": "{ meta { name } }"}, {"query": "{ meta { name } }"}]"#)
                .await
                .unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body,
            json!({ "errors": [{
                "message": "Batch requests are not supported. Send one query per request."
            }] })
        );
    }

    #[tokio::test]
    async fn valid_request_is_not_rejected() {
        assert!(reject(r#"{"query": "{ meta { name } }"}"#).await.is_none());
    }

    #[test]
    fn payload_too_large_keeps_its_status() {
        let resp = RequestRejection(ParseRequestError::PayloadTooLarge).into_response();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
