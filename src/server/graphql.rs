//! GraphQL handler.

use async_graphql::{dataloader::DataLoader, http::GraphiQLSource};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    Extension,
    extract::OriginalUri,
    response::{Html, IntoResponse},
};

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{disease::DiseaseLoader, disease_hpo::DiseasePhenotypeLoader, hpo::HpoLoader},
    schema::ApiSchema,
};

pub async fn graphiql(OriginalUri(uri): OriginalUri) -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint(uri.path()).finish())
}

pub async fn handler(
    Extension(schema): Extension<ApiSchema>,
    Extension(ch): Extension<ClickHouse>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let diseases = DataLoader::new(DiseaseLoader::new(ch.clone()), tokio::spawn);
    let hpos = DataLoader::new(HpoLoader::new(ch.clone()), tokio::spawn);
    let phenotypes = DataLoader::new(DiseasePhenotypeLoader::new(ch.clone()), tokio::spawn);
    schema
        .execute(req.into_inner().data(diseases).data(hpos).data(phenotypes))
        .await
        .into()
}
