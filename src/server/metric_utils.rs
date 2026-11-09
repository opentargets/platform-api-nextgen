use axum_prometheus::metrics;

const API_PREFIX: &str = "platform_api_";

pub fn cache_miss() {
    metrics::counter!(
        description: "Number of requests not served from cache.",
        format!("{API_PREFIX}cache_missed_total")
    )
    .increment(1);
}
pub fn cache_hit() {
    metrics::counter!(
        description: "Number of requests served from cache.",
        format!("{API_PREFIX}cache_hit_total")
    )
    .increment(1);
}
pub fn cache_registration() {
    metrics::counter!(
        description: "Number of requests registered in the cache.",
        format!("{API_PREFIX}cache_registration_total")
    )
    .increment(1);
}
pub fn db_queries(database: String) {
    metrics::counter!(
        description: "Number of queries to databases.",
        format!("{API_PREFIX}db_query_total"),
        "database" => database
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
