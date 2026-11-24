use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, Enum, Error, Result, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        interaction_resource::InteractionResource,
        target::{Target, TargetLoader},
    },
    query::{
        paginate::Paged,
        sort::SortDirection,
        sql::{Column, Filter, Op, OrderBy, PageSpec, Table, Value, ValueKind, build, columns},
    },
};

// ---- models ----

/// Source database reporting the molecular interaction.
#[derive(
    Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Enum, Deserialize_repr, Serialize_repr,
)]
#[repr(i8)]
#[graphql(rename_items = "lowercase")]
pub enum InteractionSourceDatabase {
    /// IntAct: <https://www.ebi.ac.uk/intact/>.
    Intact = 1,
    /// Reactome: <https://reactome.org/>.
    Reactome = 2,
    /// Signor: <https://signor.uniroma2.it/>.
    Signor = 3,
    /// String: <https://string-db.org/>.
    String = 4,
}

/// Taxonomic annotation of the first interaction participant.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct InteractionSpeciesA {
    /// Short mnemonic name of the associated species.
    mnemonic: String,
    /// Scientific name of the associated species.
    scientific_name: String,
    /// NCBI taxon ID of the associated species.
    taxon_id: u32,
}

/// Taxonomic annotation of the second interaction participant.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct InteractionSpeciesB {
    /// Short mnemonic name of the associated species.
    mnemonic: String,
    /// Scientific name of the associated species.
    scientific_name: Option<String>,
    /// NCBI taxon ID of the associated species.
    taxon_id: Option<u32>,
}

/// Evidence supporting molecular interactions between targets. Contains detailed information about
/// how the interaction was detected, the experimental context, and supporting publications.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct InteractionEvidence {
    /// Score indicating the confidence or strength of the interaction evidence.
    evidence_score: Option<f64>,
    /// Molecular Interactions (MI) identifier for the expansion method used [bioregistry:mi].
    expansion_method_mi_identifier: Option<String>,
    /// Short name of the method used to expand the interaction dataset.
    expansion_method_short_name: Option<String>,
    /// Scientific name of the host organism in which the interaction was observed.
    host_organism_scientific_name: Option<String>,
    /// NCBI taxon ID of the host organism.
    host_organism_tax_id: Option<u32>,
    /// Molecular Interactions (MI) identifier for the interaction detection method
    /// [bioregistry:mi].
    interaction_detection_method_mi_identifier: String,
    /// Short name of the method used to detect the interaction.
    interaction_detection_method_short_name: String,
    /// Unique identifier for the interaction evidence entry at the source.
    interaction_identifier: Option<String>,
    #[graphql(skip)]
    #[allow(dead_code)]
    /// Databases providing the interaction evidence.
    interaction_resources: InteractionResource,
    /// Molecular Interactions (MI) identifier for the type of interaction [bioregistry:mi].
    interaction_type_mi_identifier: Option<String>,
    /// Short name of the interaction type.
    interaction_type_short_name: Option<String>,
    /// Detection method used to identify participant A in the interaction.
    participant_detection_method_a: Vec<ParticipantDetectionMethod>,
    /// Detection method used to identify participant B in the interaction.
    participant_detection_method_b: Vec<ParticipantDetectionMethod>,
    /// PubMed ID of the publication supporting the interaction evidence.
    pubmed_id: Option<String>,
}

/// Detection method used to identify participants in the interaction.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantDetectionMethod {
    /// Molecular Interactions (MI) identifier for participant B's detection method
    /// [bioregistry:mi].
    mi_identifier: Option<String>,
    /// Short name of participant A's detection method.
    short_name: Option<String>,
}

/// Molecular interactions reporting experimental or functional interactions between this target and
/// other molecules. This dataset contains pair-wise interactions deposited in several databases
/// capturing: physical interactions (e.g. IntAct), directional interactions (e.g. Signor), pathway
/// relationships (e.g. Reactome) or functional interactions (e.g. STRINGdb).
// Rows are decoded by position, so field order must match the column order of
// `src/sql/interactions.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Interaction {
    /// Open Targets target identifier of the first molecule (target A) in the interaction
    /// [bioregistry:ensembl]. Resolved by the `targetA` field.
    #[graphql(skip)]
    target_a: String,
    #[allow(dead_code, reason = "read only to keep the column order")]
    #[graphql(skip)]
    idx: u32,
    /// Identifier for target A in source.
    int_a: String,
    /// Open Targets target identifier of the second molecule (target B) in the interaction
    /// [bioregistry:ensembl]. Resolved by the `targetB` field.
    #[graphql(skip)]
    target_b: Option<String>,
    /// Identifier for target B in source.
    int_b: String,
    /// Biological role of target A in the interaction.
    int_a_biological_role: String,
    /// Biological role of target B in the interaction.
    int_b_biological_role: String,
    /// Scoring or confidence value assigned to the interaction.
    score: Option<f64>,
    /// Number of interaction occurrences reported in the source databases.
    count: u8,
    /// Source database reporting the molecular interaction.
    source_database: InteractionSourceDatabase,
    /// Taxonomic annotation of target A.
    species_a: InteractionSpeciesA,
    /// Taxonomic annotation of target B.
    species_b: InteractionSpeciesB,
    /// Evidence supporting the molecular interaction.
    evidences: Vec<InteractionEvidence>,
}

impl Table for Interaction {
    type Col = InteractionCol;
    const NAME: &'static str = include_str!("../sql/interactions.sql");
    // Unique within one target, which every query filters on.
    const TIE_BREAKER: &'static str = "idx";
}

columns!(InteractionCol {
    TargetA        => "targetA"                : Str,
    Score          => "score"                  : F64,
    // Scores are in [0, 1] and only Reactome/Signor have none. Treating a missing score as 1
    // those interactions passing every threshold, as before.
    ScoreOrMax     => "ifNull(score, 1)"       : F64,
    // Enum8 in ClickHouse, filtered on its number.
    SourceDatabase => "toInt8(sourceDatabase)" : I64,
});

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    target_id: String,
    /// `f64::to_bits` of the score threshold, so the key is `Eq + Hash`.
    score_threshold: Option<u64>,
    source_database: Option<InteractionSourceDatabase>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(
        target_id: String,
        score_threshold: Option<f64>,
        source_database: Option<InteractionSourceDatabase>,
        index: u32,
        size: u32,
    ) -> Self {
        Self {
            target_id,
            score_threshold: score_threshold.map(f64::to_bits),
            source_database,
            index,
            size,
        }
    }

    fn to_spec(&self) -> Result<PageSpec<Interaction>> {
        use InteractionCol as C;
        // The key of the EmbeddedRocksDB `interaction` table.
        let mut filters = vec![
            Filter::new(C::TargetA, Op::Eq, Value::Str(self.target_id.clone()))
                .map_err(Error::new)?,
        ];
        if let Some(bits) = self.score_threshold {
            filters.push(
                Filter::new(C::ScoreOrMax, Op::Ge, Value::F64(f64::from_bits(bits)))
                    .map_err(Error::new)?,
            );
        }
        if let Some(db) = self.source_database {
            filters.push(
                Filter::new(C::SourceDatabase, Op::Eq, Value::I64(i64::from(db as i8)))
                    .map_err(Error::new)?,
            );
        }
        Ok(PageSpec {
            filters,
            // Always highest score first. ClickHouse puts NULL scores last.
            order_by: vec![OrderBy {
                col: C::Score,
                dir: SortDirection::Descending,
            }],
            offset: u64::from(self.index) * u64::from(self.size),
            limit: u64::from(self.size),
        })
    }
}

// ---- loader ----

/// One row of the batched query: the page of results for `keys[key]`.
#[derive(Debug, Clone, Row, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRow {
    key: usize,
    interactions: Paged<Interaction>,
}

#[derive(From)]
pub struct InteractionLoader {
    ch: ClickHouse,
}

impl InteractionLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<QueryParams> for InteractionLoader {
    type Value = Paged<Interaction>;
    type Error = Error;

    async fn load(&self, keys: &[QueryParams]) -> Result<HashMap<QueryParams, Self::Value>> {
        let rendered = keys
            .iter()
            .map(|k| k.to_spec().map(|spec| spec.render()))
            .collect::<Result<Vec<_>>>()?;

        let rows = build(self.ch.client(), &rendered)
            .fetch_all::<BatchRow>()
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| (keys[row.key].clone(), row.interactions))
            .collect())
    }
}

/// # Errors
/// If the `ClickHouse` query fails.
pub async fn load_interactions(ctx: &Context<'_>, key: QueryParams) -> Result<Paged<Interaction>> {
    Ok(ctx
        .data_unchecked::<DataLoader<InteractionLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}

#[ComplexObject]
impl Interaction {
    /// Target A of the interaction.
    async fn target_a(&self, ctx: &Context<'_>) -> Result<Option<Target>> {
        ctx.data_unchecked::<DataLoader<TargetLoader>>()
            .load_one(self.target_a.clone())
            .await
    }

    /// Target B of the interaction. Null if it isn't a known target.
    async fn target_b(&self, ctx: &Context<'_>) -> Result<Option<Target>> {
        match &self.target_b {
            Some(target_id) => {
                ctx.data_unchecked::<DataLoader<TargetLoader>>()
                    .load_one(target_id.clone())
                    .await
            }
            None => Ok(None),
        }
    }
}
