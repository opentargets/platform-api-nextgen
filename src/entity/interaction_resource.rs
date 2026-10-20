use async_graphql::{Context, Object, SimpleObject};
use clickhouse::Row;
use serde::Deserialize;

use crate::datasource::clickhouse::ClickHouse;

// ---- models ----

/// Databases providing evidence for the interaction.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct InteractionResource {
    /// Version of the source database providing interaction evidence.
    database_version: String,
    /// Name of the source database reporting the interaction evidence.
    source_database: String,
}

// ---- resolvers ----

#[derive(Default)]
pub struct InteractionResourceQuery;

#[Object]
impl InteractionResourceQuery {
    /// List of molecular interaction resources and their versions.
    async fn interaction_resources(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<InteractionResource>> {
        let ch = ctx.data_unchecked::<ClickHouse>();
        let sql = "SELECT DISTINCT interactionResources.databaseVersion, interactionResources.sourceDatabase
        FROM interaction_evidence_log";
        Ok(ch.query(sql).fetch_all::<InteractionResource>().await?)
    }
}
