//! OpenSearch connector.

use opensearch::{
    Error, OpenSearch as Client,
    http::{
        Url,
        transport::{SingleNodeConnectionPool, TransportBuilder},
    },
};

use crate::config::Config;

#[derive(Clone)]
pub struct OpenSearch(Client);

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
                Self(client)
            }
            Err(e) => {
                panic!("failed to create opensearch transport: {e}");
            }
        }
    }

    #[must_use]
    pub fn client(&self) -> &Client { &self.0 }

    /// Readiness probe.
    ///
    /// # Errors
    /// Returns an error if the OpenSearch client cannot ping the server or if the server returns
    /// an error status code.
    pub async fn health(&self) -> Result<(), Error> {
        self.0.ping().send().await?.error_for_status_code()?;
        Ok(())
    }
}
