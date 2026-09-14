//! [`Product`] represents the different products served by the API (platform/ppp).

use async_graphql::{
    EmptyMutation, EmptySubscription, Enum, ObjectType, Request, Response, Schema,
    dataloader::DataLoader,
};
use serde::{Deserialize, Serialize};
use strum::Display;

use crate::{
    config::Config,
    datasource::{clickhouse::ClickHouse, opensearch::OpenSearch},
    entity::meta::Meta,
};

// ---- Selection of the product the API will serve ------------------------------------------------

/// The different product flavors served by the API (platform/ppp).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Display, Enum, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Flavor {
    /// Open Targets Platform Public version.
    Platform,
    /// Open Targets Partner Preview Platform version.
    Ppp,
}

// Require exactly one of `product-platform` or `product-ppp` to be enabled.
#[cfg(all(feature = "product-platform", feature = "product-ppp"))]
compile_error!("`product-*` compile flags are mutually exclusive");
#[cfg(not(any(feature = "product-platform", feature = "product-ppp",)))]
compile_error!("enable exactly one of `product-platform` / `product-ppp`");

#[rustfmt::skip] #[cfg(feature = "product-platform")] pub mod platform;
#[rustfmt::skip] #[cfg(feature = "product-ppp")]      pub mod ppp;
#[rustfmt::skip] #[cfg(feature = "product-platform")] pub type CurrentProduct = platform::Platform;
#[rustfmt::skip] #[cfg(feature = "product-ppp")]      pub type CurrentProduct = ppp::Ppp;

// -------------------------------------------------------------------------------------------------

/// Represents a product served by the API.
pub trait Product {
    /// The name of the product.
    const NAME: &'static Flavor;
    /// The GraphQL query type for this product.
    type Query: ObjectType + Default + 'static;
    /// Prepares the request for this product.
    fn prepare_request(req: Request, ch: &ClickHouse) -> Request;
}

/// The API schema for the current product.
pub type ApiSchema = Schema<<CurrentProduct as Product>::Query, EmptyMutation, EmptySubscription>;

/// The API that will be served by this application.
///
/// The [`OpenSearch`] client does not need to be stored, because it is used only in the schema.
/// There are no [`DataLoader`]s using OpenSearch yet. If later on we need it in `DataLoader`s,
/// we just need to add it here and pass it by value into [`Api::new()`].
#[derive(Clone)]
pub struct Api {
    /// The API schema for the current product.
    schema: ApiSchema,
    /// The ClickHouse client for this API.
    ch: ClickHouse,
}

impl Api {
    /// Creates a new [`Api`] instance with the given configuration and clients.
    #[must_use]
    pub fn new(config: &Config, ch: ClickHouse, os: &OpenSearch) -> Self {
        let schema = Schema::build(
            <CurrentProduct as Product>::Query::default(),
            EmptyMutation,
            EmptySubscription,
        )
        .limit_depth(config.max_depth)
        .limit_complexity(config.max_complexity)
        .data(ch.clone())
        .data(os.clone())
        .data(Meta::new(config))
        .finish();
        Self { schema, ch }
    }

    /// Executes the given request against the API schema.
    pub async fn execute(&self, req: Request) -> Response {
        self.schema
            .execute(CurrentProduct::prepare_request(req, &self.ch))
            .await
    }
}

/// Maximum number of items to batch in a single request.
pub const MAX_BATCH_SIZE: usize = 10_000;

/// Creates a [`DataLoader`] for the given type, using the given [`ClickHouse`] client.
pub fn loader<L: From<ClickHouse>>(ch: &ClickHouse) -> DataLoader<L> {
    DataLoader::new(L::from(ch.clone()), tokio::spawn).max_batch_size(MAX_BATCH_SIZE)
}
