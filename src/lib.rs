pub mod config;
pub mod datasource;
pub mod entity;
pub mod error;
pub mod logging;
pub mod plugin;
pub mod product;
pub mod query;
pub mod server;

use std::sync::Arc;

use config::Config;
use datasource::{clickhouse::ClickHouse, opensearch::OpenSearch};
use plugin::registry::PluginRegistry;

use crate::product::Api;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub plugin_registry: PluginRegistry,
    pub api: Api,
    pub http: reqwest::Client,
}

impl AppState {
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            plugin_registry: PluginRegistry::new(&config),
            api: Api::new(&config, ClickHouse::new(&config), &OpenSearch::new(&config)),
            http: reqwest::Client::new(),
            config: Arc::new(config),
        }
    }
}
