use std::sync::Arc;

use clap::Parser;
use platform_api::{
    AppState,
    config::{Args, Config},
    datasource::{clickhouse::ClickHouse, opensearch::OpenSearch},
    entity::{disease::disease_cache, hpo::hpo_cache, study::study_cache},
    logging,
    plugin::registry::PluginRegistry,
    schema, server,
};

#[tokio::main]
async fn main() {
    let config = Config::load(Args::parse().config.as_deref());
    logging::init(&config);
    tracing::info!("configuration:\n{config:#?}");

    let app_state = AppState {
        clickhouse: ClickHouse::new(&config),
        opensearch: OpenSearch::new(&config),
        plugin_registry: PluginRegistry::new(&config),
        http: reqwest::Client::new(),
        config: Arc::new(config),
        disease_cache: disease_cache(),
        hpo_cache: hpo_cache(),
        study_cache: study_cache(),
    };

    let schema = schema::build_schema(&app_state);
    server::serve::serve(app_state, schema).await;
}
