pub mod config;
pub mod datasource;
pub mod entity;
pub mod error;
pub mod logging;
pub mod plugin;
pub mod query;
pub mod server;

use std::sync::Arc;

use config::Config;
use datasource::{clickhouse::ClickHouse, opensearch::OpenSearch};
use plugin::registry::PluginRegistry;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub clickhouse: ClickHouse,
    pub opensearch: OpenSearch,
    pub plugin_registry: PluginRegistry,
    pub http: reqwest::Client,
}

impl AppState {
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            plugin_registry: PluginRegistry::new(&config),
            http: reqwest::Client::new(),
            clickhouse: ClickHouse::new(&config),
            opensearch: OpenSearch::new(&config),
            config: Arc::new(config),
        }
    }
}
