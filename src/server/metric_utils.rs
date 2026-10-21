use axum_prometheus::metrics;

const API_PREFIX: &str = "platform_api_";

pub fn cache_miss() {
    metrics::counter!(
        description: "Number of requests not served from cache.",
        format!("{API_PREFIX}graphql_cache_miss")
    )
    .increment(1);
}
pub fn cache_hit() {
    metrics::counter!(
        description: "Number of requests served from cache.",
        format!("{API_PREFIX}graphql_cache_hit")
    )
    .increment(1);
}
pub fn clickhouse_queries() {
    metrics::counter!(
        description: "Number of queries to Clickhouse database.",
        format!("{API_PREFIX}ch_query_total")
    )
    .increment(1);
}
pub fn opensearch_queries() {
    metrics::counter!(
        description: "Number of queries to Opensearch database.",
        format!("{API_PREFIX}os_query_total")
    )
    .increment(1);
}
pub fn graphql_request() {
    metrics::counter!(
        description: "Number of requets to the GraphQL endpoint.",
        format!("{API_PREFIX}graphql_requests_total")
    )
    .increment(1);
}
