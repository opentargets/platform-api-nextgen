//! GraphQL handler.

use async_graphql::{
    EmptyMutation, EmptySubscription, MergedObject, Schema, dataloader::DataLoader,
    http::GraphiQLSource,
};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    Extension,
    extract::OriginalUri,
    response::{Html, IntoResponse},
};
use tracing::Instrument;

use crate::{
    AppState,
    datasource::{clickhouse::ClickHouse, opensearch::OpenSearch},
    entity::{
        disease::{DiseaseLoader, DiseaseQuery},
        disease_hpo::DiseasePhenotypeLoader,
        hpo::HpoLoader,
        meta::{Meta, MetaQuery},
        search::SearchQuery,
        search_facet::FacetQuery,
        study::{StudyLoader, StudyQuery},
        target::{TargetLoader, TargetQuery},
    },
};

const MAX_BATCH_SIZE: usize = 10_000;

pub async fn graphiql(OriginalUri(uri): OriginalUri) -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint(uri.path()).finish())
}

pub async fn handler(
    Extension(schema): Extension<ApiSchema>,
    Extension(ch): Extension<ClickHouse>,
    Extension(os): Extension<OpenSearch>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let diseases = DataLoader::new(DiseaseLoader::new(ch.clone()), tokio::spawn)
        .max_batch_size(MAX_BATCH_SIZE);
    let hpos =
        DataLoader::new(HpoLoader::new(ch.clone()), tokio::spawn).max_batch_size(MAX_BATCH_SIZE);
    let studies =
        DataLoader::new(StudyLoader::new(ch.clone()), tokio::spawn).max_batch_size(MAX_BATCH_SIZE);
    let phenotypes = DataLoader::new(DiseasePhenotypeLoader::new(ch.clone()), tokio::spawn)
        .max_batch_size(MAX_BATCH_SIZE);
    let targets =
        DataLoader::new(TargetLoader::new(ch.clone()), tokio::spawn).max_batch_size(MAX_BATCH_SIZE);

    let inner = req.into_inner();
    let span = tracing::info_span!(
        "graphql_request",
        op = inner.operation_name.as_deref().unwrap_or("anonymous")
    );

    schema
        .execute(
            inner
                .data(diseases)
                .data(hpos)
                .data(phenotypes)
                .data(studies)
                .data(targets)
                .data(os),
        )
        .instrument(span)
        .await
        .into()
}

#[derive(MergedObject, Default)]
pub struct Query(
    MetaQuery,    // API data (version, data release, product, etc.)
    SearchQuery,  // Search bar functionality
    FacetQuery,   // Facet search for AOTF
    DiseaseQuery, // Diseases
    StudyQuery,   // Studies
    TargetQuery,  // Targets
);

pub type ApiSchema = Schema<Query, EmptyMutation, EmptySubscription>;

#[must_use]
pub fn build_schema(state: &AppState) -> ApiSchema {
    Schema::build(Query::default(), EmptyMutation, EmptySubscription)
        // .extension(async_graphql::extensions::Tracing)
        .limit_depth(state.config.max_depth)
        .limit_complexity(state.config.max_complexity)
        .data(state.clickhouse.clone())
        .data(state.opensearch.clone())
        .data(Meta::new(&state.config))
        .finish()
}
