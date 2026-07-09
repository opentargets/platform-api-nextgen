//! GraphQL schema definition and builder.

use async_graphql::{
    EmptyMutation, EmptySubscription, MergedObject, OutputType, Schema, SimpleObject,
};

use crate::{
    AppState,
    entity::{
        disease::{Disease, DiseaseQuery},
        meta::{Meta, MetaQuery},
        study::{Study, StudyQuery},
    },
};

#[derive(Debug, SimpleObject)]
#[graphql(concrete(name = "DiseasePage", params(Disease)))]
#[graphql(concrete(name = "StudyPage", params(Study)))]
pub struct Paged<T: OutputType> {
    pub total: u64,
    pub items: Vec<T>,
}

#[derive(MergedObject, Default)]
pub struct Query(DiseaseQuery, MetaQuery, StudyQuery);

pub type ApiSchema = Schema<Query, EmptyMutation, EmptySubscription>;

#[must_use]
pub fn build_schema(state: &AppState) -> ApiSchema {
    Schema::build(Query::default(), EmptyMutation, EmptySubscription)
        .data(state.clickhouse.clone())
        .data(Meta::new(&state.config))
        .finish()
}
