//! `meta` entity: Metadata for the API and its underlying data sources.

use async_graphql::{ComplexObject, Context, Object, SimpleObject};
use serde_json::{Value, json};

use crate::{
    config::Config,
    datasource::opensearch::OpenSearch,
    product::{CurrentProduct, Flavor, Product},
};

// ---- models ----

/// Version information for the API and its underlying data sources.
#[derive(Debug, Clone, SimpleObject)]
pub struct Version {
    /// Year of the version, e.g. `23`.
    pub year: String,
    /// Month of the version, e.g. `06`.
    pub month: String,
    /// Revision of the version, e.g. `0`. This field is optional for data releases and may be
    /// omitted, but not for the API version, which always has a revision even if it is `0`.
    pub revision: String,
}

/// Metadata for the API and its underlying data sources.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(complex)]
pub struct Meta {
    /// Name of the API.
    name: String,
    /// Open Targets platform API product name.
    pub product: &'static Flavor,
    /// API version information.
    api_version: Version,
    /// Data release version information.
    data_version: Version,
    /// Data release namespace.
    data_namespace: String,
}

// ---- retrievers ----

fn split_version(v: &str) -> (String, String, String) {
    let mut parts = v.split('.').map(str::to_string);
    (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    )
}

fn get_downloads_query() -> Value {
    json!({
      "query": { "ids": { "values": ["croissant"] } },
      "size": 3000,
      "_source": true,
      "track_total_hits": true
    })
}

fn parse_downloads(response: &Value) -> Option<String> {
    response
        .get("hits")
        .and_then(|hits| hits.get("hits"))
        .and_then(Value::as_array)
        .and_then(|hits| hits.first())
        .and_then(|hit| hit.get("_source"))
        .and_then(|s| serde_json::to_string(&s).ok())
}

#[ComplexObject]
impl Meta {
    /// Platform datasets described following MLCroissant metadata format.
    async fn downloads(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<String>> {
        let os = ctx.data::<OpenSearch>()?;
        let index_name = ["downloads"];
        let res = os.search(&index_name, get_downloads_query()).await?;
        let source = parse_downloads(&res);
        Ok(source)
    }
}

impl Meta {
    #[must_use]
    pub fn new(config: &Config) -> Self {
        let api_version = env!("CARGO_PKG_VERSION").to_string();
        let (api_year, api_month, api_revision) = split_version(&api_version);
        let (data_year, data_month, data_revision) = split_version(&config.data_release);
        let product = <CurrentProduct as Product>::NAME;
        let name = format!("Open Targets {product} API {api_version}");
        let data_namespace = format!("{product}{data_year}{data_month}");

        Self {
            product,
            name,
            data_namespace,
            api_version: Version {
                year: api_year,
                month: api_month,
                revision: api_revision,
            },
            data_version: Version {
                year: data_year,
                month: data_month,
                revision: data_revision,
            },
        }
    }
}

// ---- resolvers ----

#[derive(Default)]
pub struct MetaQuery;

#[Object]
impl MetaQuery {
    #[allow(clippy::unused_async)]
    async fn meta(&self, ctx: &Context<'_>) -> async_graphql::Result<Meta> {
        Ok(ctx.data::<Meta>()?.clone())
    }
}
