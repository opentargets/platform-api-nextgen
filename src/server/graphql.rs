//! GraphQL handler.

use async_graphql::{dataloader::DataLoader, http::GraphiQLSource};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    Extension,
    extract::OriginalUri,
    response::{Html, IntoResponse},
};
use tracing::Instrument;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        disease::{DiseaseCache, DiseaseLoader},
        disease_hpo::DiseasePhenotypeLoader,
        hpo::HpoLoader,
    },
    schema::ApiSchema,
};

pub async fn graphiql(OriginalUri(uri): OriginalUri) -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint(uri.path()).finish())
}

pub async fn handler(
    Extension(schema): Extension<ApiSchema>,
    Extension(ch): Extension<ClickHouse>,
    Extension(disease_cache): Extension<DiseaseCache>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let diseases = DataLoader::new(
        DiseaseLoader::new(ch.clone(), disease_cache.clone()),
        tokio::spawn,
    );
    let hpos = DataLoader::new(HpoLoader::new(ch.clone()), tokio::spawn);
    let phenotypes = DataLoader::new(DiseasePhenotypeLoader::new(ch.clone()), tokio::spawn);
    let inner = req.into_inner();
    let span = tracing::info_span!(
        "graphql_request",
        op = inner.operation_name.as_deref().unwrap_or("anonymous")
    );
    schema
        .execute(inner.data(diseases).data(hpos).data(phenotypes))
        .instrument(span)
        .await
        .into()
}
