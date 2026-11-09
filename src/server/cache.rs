#![allow(clippy::cast_precision_loss)]

use std::{
    hash::{Hash, Hasher},
    sync::LazyLock,
};

use axum::{
    body::{Body, Bytes},
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use moka::future::Cache;
use serde_json::Value;

use crate::{config::CACHE_REQUEST_SIZE, server::metric_utils};

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut val = bytes as f64;
    let mut i = 0;
    while val >= 1024.0 && i < UNITS.len() - 1 {
        val /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{val:.1} {}", UNITS[i])
    }
}

/// Parse body to JSON.
fn parse(body: &[u8]) -> Option<Value> { serde_json::from_slice(body).ok() }

/// Removes comments and whitespace from a GraphQL query by parsing it.
fn normalize_query(v: &mut Value) {
    if let Some(Value::String(q)) = v.get("query")
        && let Ok(doc) = graphql_parser::query::parse_query::<String>(q)
    {
        v["query"] = Value::String(doc.to_string());
    }
}

/// Returns the name of the single named operation in the GraphQL document, if any.
///
/// Documents with more than one named operation are ambiguous, so no name is returned.
fn document_operation_name(query: &str) -> Option<String> {
    use std::borrow::Borrow;

    use graphql_parser::query::{Definition, OperationDefinition};

    let doc = graphql_parser::query::parse_query::<String>(query).ok()?;
    let mut names = doc.definitions.iter().filter_map(|def| match def {
        Definition::Operation(OperationDefinition::Query(op)) => op.name.as_ref(),
        Definition::Operation(
            OperationDefinition::Mutation(_)
            | OperationDefinition::Subscription(_)
            | OperationDefinition::SelectionSet(_),
        )
        | Definition::Fragment(_) => None,
    });

    let name = names.next()?;
    if names.next().is_some() {
        return None;
    }
    Some(Borrow::<str>::borrow(name).to_string())
}

/// Extracts the GraphQL operation name from the request body.
///
/// The `operationName` field takes precedence. When it is missing, the name of the
/// single named operation in the query document is used instead.
fn query_name(v: &Value) -> String {
    if let Some(Value::String(name)) = v.get("operationName")
        && !name.is_empty()
    {
        return name.clone();
    }
    match v.get("query") {
        Some(Value::String(query)) => document_operation_name(query).unwrap_or_default(),
        _ => String::new(),
    }
}

static CACHE: LazyLock<Cache<u64, (u16, Bytes)>> = LazyLock::new(|| {
    Cache::builder()
        .weigher(|_k, (_status, body): &(u16, Bytes)| body.len().try_into().unwrap_or(u32::MAX))
        .max_capacity(CACHE_REQUEST_SIZE)
        .build()
});

pub async fn post_cache(req: Request, next: Next) -> Response {
    // only cache POST requests
    if req.method() != axum::http::Method::POST {
        return next.run(req).await;
    }

    // process the request body and build the cache key
    let (parts, body) = req.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let mut parsed = parse(&bytes);
    let query_name = parsed.as_ref().map(query_name).unwrap_or_default();
    let key = {
        let normalized = parsed.as_mut().map(|v| {
            normalize_query(v);
            v.to_string().into_bytes()
        });

        let to_hash = normalized.as_deref().unwrap_or(bytes.as_ref());
        let mut h = std::collections::hash_map::DefaultHasher::new();
        to_hash.hash(&mut h);
        h.finish()
    };

    // increment the counter metrics for graphql post requests
    metric_utils::graphql_request(&query_name);

    // on cache hit: return cached response
    if let Some((status, body)) = CACHE.get(&key).await {
        tracing::debug!(
            "cache hit: key={key} status={status} size={}",
            human(body.len() as u64)
        );
        metric_utils::cache_hit(&query_name);
        return build(status, body);
    }

    // on cache miss: rebuild request, run handler
    tracing::debug!("cache miss: key={key}");
    metric_utils::cache_miss(&query_name);
    let req = Request::from_parts(parts, Body::from(bytes));
    let resp = next.run(req).await;
    let status = resp.status().as_u16();
    let (_, body) = resp.into_parts();
    let Ok(out) = axum::body::to_bytes(body, usize::MAX).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    // only cache successful responses with no errors
    if status == 200
        && serde_json::from_slice::<serde_json::Value>(&out)
            .ok()
            .and_then(|v| {
                v.get("errors")
                    .map(|e| e.as_array().is_none_or(Vec::is_empty))
            })
            .unwrap_or(true)
    {
        let size = out.len();
        CACHE.insert(key, (status, out.clone())).await;
        metric_utils::cache_registration(&query_name);
        CACHE.run_pending_tasks().await;
        tracing::debug!(
            "cache store: key={key} size={} used={}/{}",
            human(size as u64),
            human(CACHE.weighted_size()),
            human(CACHE_REQUEST_SIZE),
        );
    } else {
        tracing::debug!("cache store: key={key} skipped");
    }

    // rebuild response
    build(status, out)
}

fn build(status: u16, body: Bytes) -> Response {
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        body,
    )
        .into_response()
}
