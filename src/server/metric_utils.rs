use axum_prometheus::metrics;

const API_PREFIX: &str = "platform_api_";

pub fn cache_miss() { metrics::counter!(format!("{API_PREFIX}graphql_cache_miss")).increment(1); }
pub fn cache_hit() { metrics::counter!(format!("{API_PREFIX}graphql_cache_hit")).increment(1); }
pub fn clickhouse_queries() {
    metrics::counter!(format!("{API_PREFIX}graphql_cache_hit")).increment(1);
}
pub fn opensearch_queries() {
    metrics::counter!(format!("{API_PREFIX}graphql_cache_hit")).increment(1);
}
