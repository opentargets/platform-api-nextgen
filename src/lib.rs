pub mod config;
pub mod datasource;
pub mod entity;
pub mod error;
pub mod logging;
pub mod plugin;
pub mod query;
pub mod schema;
pub mod server;

use std::sync::Arc;

use config::Config;
use datasource::{clickhouse::ClickHouse, opensearch::OpenSearch};
use plugin::registry::PluginRegistry;

use crate::entity::disease::DiseaseCache;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub clickhouse: ClickHouse,
    pub opensearch: OpenSearch,
    pub plugin_registry: PluginRegistry,
    pub http: reqwest::Client,
    pub disease_cache: DiseaseCache,
}
