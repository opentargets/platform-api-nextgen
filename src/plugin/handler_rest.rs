use axum::{
    body::{Body, Bytes},
    extract::{Path, RawQuery, State},
    http::{HeaderMap, HeaderName},
    response::Response,
};
use reqwest::{
    Method, StatusCode,
    header::{
        CONNECTION, PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING,
        UPGRADE,
    },
};

use crate::AppState;

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

/// Handler for REST plugins.
///
/// Routes incoming requests to the appropriate plugin and handles the response.
///
/// # Errors
/// Returns a `502 Bad Gateway` error if the request could not be sent to the plugin
/// or if the responsef from the plugin could not be parsed.
pub async fn handle(
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
    let mut builder = Response::builder().status(status);
    if let Some(headers) = builder.headers_mut() {
        for (name, value) in upstream.headers() {
            if !HOP_BY_HOP.contains(name) {
                headers.insert(name, value.clone());
            }
        }
    }
    let body_bytes = upstream
        .bytes()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let response = builder
        .body(Body::from(body_bytes))
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(response)
}
