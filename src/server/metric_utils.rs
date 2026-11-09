use axum_prometheus::metrics;

const API_PREFIX: &str = "platform_api_";

pub fn cache_miss(query_name: &str) {
    metrics::counter!(
        description: "Number of requests not served from cache.",
        format!("{API_PREFIX}cache_missed_total"),
        "query_name" => query_name.to_string()
    )
    .increment(1);
}
pub fn cache_hit(query_name: &str) {
    metrics::counter!(
        description: "Number of requests served from cache.",
        format!("{API_PREFIX}cache_hit_total"),
        "query_name" => query_name.to_string()
    )
    .increment(1);
}
pub fn cache_registration(query_name: &str) {
    metrics::counter!(
        description: "Number of requests registered in the cache.",
        format!("{API_PREFIX}cache_registration_total"),
        "query_name" => query_name.to_string()
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
pub fn graphql_request(query_name: &str) {
    metrics::counter!(
        description: "Number of requets to the GraphQL endpoint.",
        format!("{API_PREFIX}graphql_requests_total"),
        "query_name" => query_name.to_string()
    )
    .increment(1);
}
