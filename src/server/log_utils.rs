//! Client IP extraction for requests handled by the API.

use axum::http::HeaderMap;

/// Header set by the reverse proxy in front of the API with the proxy chain.
const FORWARDED_FOR: &str = "X-Forwarded-For";

/// Extracts the client IP address from the request headers.
///
/// Returns `unknown` when the header is absent or empty.
pub fn client_ip(headers: &HeaderMap) -> String {
    let Some(forwarded) = headers.get(FORWARDED_FOR) else {
        return "unknown".to_string();
    };
    let Ok(forwarded) = forwarded.to_str() else {
        return "unknown".to_string();
    };

    let chain: Vec<&str> = forwarded
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    match chain.len() {
        // The load balancer appends its own address, so the client is the entry before it.
        0 => "unknown".to_string(),
        1 => chain[0].to_string(),
        _ => chain[chain.len() - 2].to_string(),
    }
}
