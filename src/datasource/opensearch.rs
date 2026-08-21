//! OpenSearch connector.

use opensearch::{
    Error, OpenSearch as Client, SearchParts,
    http::{
        Url,
        transport::{SingleNodeConnectionPool, TransportBuilder},
    },
};
use serde_json::Value;

use crate::config::Config;

#[derive(Clone)]
pub struct OpenSearch {
    client: Client,
    prefix: String,
}

impl OpenSearch {
    /// Create a new OpenSearch client.
    ///
    /// # Panics
    /// Panics if the OpenSearch URL is invalid or if the transport cannot be created.
    pub fn new(config: &Config) -> Self {
        let url = Url::parse(&config.opensearch_url).expect("invalid opensearch url");
        let conn_pool = SingleNodeConnectionPool::new(url);
        let transport = TransportBuilder::new(conn_pool)
            .timeout(config.opensearch_timeout)
            .build();
        match transport {
            Ok(transport) => {
                let client = Client::new(transport);
                tracing::info!(
                    "opensearch client initialized with url: {}",
                    config.opensearch_url
                );
                Self {
                    client,
                    prefix: config.data_namespace(),
                }
            }
            Err(e) => {
                panic!("failed to create opensearch transport: {e}");
            }
        }
    }

    #[must_use]
    pub fn client(&self) -> &Client { &self.client }

    /// Readiness probe.
    ///
    /// # Errors
    /// Returns an error if the OpenSearch client cannot ping the server or if the server returns
    /// an error status code.
    pub async fn health(&self) -> Result<(), Error> {
        self.client.ping().send().await?.error_for_status_code()?;
        Ok(())
    }

    /// Search for documents in the given indices using the given body.
    ///
    /// # Errors
    /// Returns an error if the request fails or the body isn't valid JSON.
    pub async fn search(&self, indices: &[&str], body: Value) -> Result<Value, Error> {
        let prefixed_indices: Vec<String> = indices
            .iter()
            .map(|i| format!("{}_{i}", self.prefix))
            .collect();
        let prefixed_indices: Vec<&str> = prefixed_indices.iter().map(String::as_str).collect();
        let resp = self
            .client
            .search(SearchParts::Index(&prefixed_indices))
            .body(body)
            .send()
            .await?
            .error_for_status_code()?;
        resp.json().await
    }
}
