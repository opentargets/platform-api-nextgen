//! ClickHouse connector.

use clickhouse::{Client, error::Result};

use crate::config::Config;

#[derive(Clone)]
pub struct ClickHouse(Client);

impl ClickHouse {
    pub fn new(config: &Config) -> Self {
        let client = Client::default()
            .with_url(&config.clickhouse_url)
            .with_database(config.data_namespace())
            // We have to disable validation until clickhouse-rs adds named tuple support,
            // see: https://github.com/ClickHouse/clickhouse-rs/issues/351
            .with_validation(false)
            .with_setting("wait_end_of_query", "1")
            .with_setting(
                "max_execution_time",
                config.clickhouse_max_execution_time.as_secs().to_string(),
            )
            .with_setting("cancel_http_readonly_queries_on_client_close", "1");
        tracing::info!(
            "clickhouse client initialized with url: {}",
            config.clickhouse_url
        );
        Self(client)
    }

    /// Returns the ClickHouse client.
    #[must_use]
    pub fn client(&self) -> &Client { &self.0 }

    /// Start a query. `?fields` expands to the row's columns, `?` binds a value,
    /// `sql::Identifier` binds table names from config.
    pub fn query(&self, sql: &str) -> clickhouse::query::Query { self.0.query(sql) }

    /// Readiness probe.
    ///
    /// # Errors
    /// This function will return an error if the database is unreachable or unresponsive.
    pub async fn health(&self) -> Result<()> { self.0.query("SELECT 1").execute().await }
}
