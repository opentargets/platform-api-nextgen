use async_graphql::{Context, Object, SimpleObject};
use clickhouse::Row;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{association::Datasource, evidence::Datatype},
};

// ---- models ----

/// List of available evidence datasources and their datatypes.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
pub struct EvidenceSource {
    /// Name of the evidence datasource.
    datasource: Datasource,
    /// Datatype/category of the evidence (e.g., Genetic association, Somatic, Literature).
    datatype: Datatype,
}

// ---- resolvers ----

#[derive(Default)]
pub struct EvidenceSourceQuery;

#[Object]
impl EvidenceSourceQuery {
    /// List of available evidence datasources and their datatypes.
    async fn association_datasources(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<EvidenceSource>> {
        let ch = ctx.data_unchecked::<ClickHouse>();
        let sql = "SELECT DISTINCT datasourceId AS datasource, datatypeId AS datatype \
                   FROM associations_otf_disease ORDER BY datasource, datatype";
        Ok(ch.query(sql).fetch_all::<EvidenceSource>().await?)
    }
}
