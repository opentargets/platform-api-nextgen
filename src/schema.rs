//! GraphQL schema definition and builder.

use async_graphql::{EmptyMutation, EmptySubscription, MergedObject, Schema};

use crate::{
    AppState,
    entity::{
        disease::DiseaseQuery,
        meta::{Meta, MetaQuery},
        search::SearchQuery,
        study::StudyQuery,
    },
};

#[derive(MergedObject, Default)]
pub struct Query(DiseaseQuery, MetaQuery, StudyQuery, SearchQuery);

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
