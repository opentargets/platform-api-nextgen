//! `meta` entity: Metadata for the API and its underlying data sources.

use async_graphql::{Context, Object, SimpleObject};

use crate::config::Config;

// ---- model ----

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
pub struct Meta {
    /// Name of the API.
    name: String,
    /// Open Targets platform API product name.
    product: String,
    /// API version information.
    api_version: Version,
    /// Data release version information.
    data_version: Version,
    /// Data release namespace.
    data_namespace: String,
    /// Platform datasets described following MLCroissant metadata format.
    downloads: Option<String>,
}

// ---- construction ----

fn split_version(v: &str) -> (String, String, String) {
    let mut parts = v.split('.').map(str::to_string);
    (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    )
}

impl Meta {
    #[must_use]
    pub fn new(config: &Config) -> Self {
        let api_version = env!("CARGO_PKG_VERSION").to_string();
        let (api_year, api_month, api_revision) = split_version(&api_version);
        let (data_year, data_month, data_revision) = split_version(&config.data_release);
        let name = format!("Open Targets {} API {}", config.product, api_version);

        Self {
            product: config.product.clone(),
            name,
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
            data_namespace: config.data_namespace(),
            downloads: None,
        }
    }
}

// ---- resolver ----

#[derive(Default)]
pub struct MetaQuery;

#[Object]
impl MetaQuery {
    #[allow(clippy::unused_async)]
    async fn meta(&self, ctx: &Context<'_>) -> async_graphql::Result<Meta> {
        Ok(ctx.data::<Meta>()?.clone())
    }
}
