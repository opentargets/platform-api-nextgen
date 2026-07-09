//! GraphQL handler.

use async_graphql::{dataloader::DataLoader, http::GraphiQLSource};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    Extension,
    extract::OriginalUri,
    response::{Html, IntoResponse},
};

use crate::{
    datasource::clickhouse::ClickHouse, entity::disease::DiseaseLoader, schema::ApiSchema,
};

pub async fn graphiql(OriginalUri(uri): OriginalUri) -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint(uri.path()).finish())
}

pub async fn handler(
    Extension(schema): Extension<ApiSchema>,
    Extension(ch): Extension<ClickHouse>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let diseases = DataLoader::new(DiseaseLoader { ch }, tokio::spawn);
    schema.execute(req.into_inner().data(diseases)).await.into()
}
