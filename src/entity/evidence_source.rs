use async_graphql::SimpleObject;
use clickhouse::Row;
use serde::Deserialize;

// ---- models ----

/// Evidence datasource and datatype metadata.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
pub struct EvidenceSource {
    /// Name of the evidence datasource.
    datasource: String,
    /// Datatype/category of the evidence (e.g., Genetic association, Somatic, Literature).
    datatype: String,
}
