use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, Enum, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::target::{Target, TargetLoader},
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
    taxon_id: u8,
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
    taxon_id: Option<u8>,
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

/// Integration of molecular interactions reporting experimental or functional interactions between
/// molecules represented as Platform targets. This dataset contains pair-wise interactions
/// deposited in several databases capturing: physical interactions (e.g. IntAct), directional
/// interactions (e.g. Signor), pathway relationships (e.g. Reactome) or functional interactions
/// (e.g. STRINGdb).
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Interaction {
    /// Open Targets target identifier of the first molecule (target A) in the interaction
    /// [bioregistry:ensembl].
    #[graphql(skip)]
    target_a: String,
    /// Identifier for target A in source.
    int_a: String,
    /// Open Targets target identifier of the second molecule (target B) in the interaction
    /// [bioregistry:ensembl].
    #[graphql(skip)]
    target_b: Option<String>,
    /// Identifier for target B in source.
    int_b: String,
    /// Biological role of target A in the interaction.
    int_a_biological_role: String,
    /// Biological role of target B in the interaction.
    int_b_biological_role: String,
    /// Scoring or confidence value assigned to the interaction.
    #[graphql(name = "score")]
    scoring: Option<f64>,
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

// ---- loaders ----

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct InteractionRow {
    pub interactions: Vec<Interaction>,
    target_a: String,
}

#[derive(From)]
pub struct InteractionLoader {
    ch: ClickHouse,
}

impl InteractionLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

fn build_query(key: &(String, Option<u64>, Option<InteractionSourceDatabase>)) -> String {
    let score_filter = match key.1 {
        Some(_) => "or(greaterOrEquals(i.scoring, q_score),isNull(i.scoring))",
        None => "true",
    };
    let database_filter = match key.2 {
        Some(_) => "equals(i.sourceDatabase, q_db)",
        None => "true",
    };
    let sort_filter = format!(
        "arrayReverseSort(i->i.scoring,arrayFilter(i->(and({score_filter},{database_filter})),t.interactions))"
    );
    format!(
        "(WITH ? as q_score, ? as q_db, SELECT {sort_filter} AS interactions, t.targetA FROM platform2609.interaction as t WHERE (in(t.targetA,(?))))"
    )
}

impl Loader<(String, Option<u64>, Option<InteractionSourceDatabase>)> for InteractionLoader {
    type Value = Vec<Interaction>;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[(String, Option<u64>, Option<InteractionSourceDatabase>)],
    ) -> Result<
        HashMap<(String, Option<u64>, Option<InteractionSourceDatabase>), Self::Value>,
        Self::Error,
    > {
        let base_queries: Vec<String> = keys.iter().map(build_query).collect();
        let mapped_keys: HashMap<String, (String, Option<u64>, Option<InteractionSourceDatabase>)> =
            keys.iter().map(|k| (k.0.clone(), k.clone())).collect();

        let full_query = keys.iter().fold(
            self.ch.query(&base_queries.join(" UNION ALL ")),
            |acc: clickhouse::query::Query, key| {
                let score = f64::from_bits(key.1.unwrap_or_default());
                match &key.2 {
                    Some(db) => acc.bind(score).bind(db).bind(&key.0),
                    None => acc.bind(score).bind("").bind(&key.0),
                }
            },
        );

        let rows: Vec<InteractionRow> = full_query.fetch_all().await?;

        let mut results: HashMap<
            (String, Option<u64>, Option<InteractionSourceDatabase>),
            Vec<Interaction>,
        > = HashMap::new();
        for row in rows {
            let id = match mapped_keys.get(&row.target_a) {
                Some(id) => id,
                None => &(String::new(), None, None),
            };
            results.entry(id.clone()).or_insert(row.interactions);
        }
        Ok(results)
    }
}

/// Loads Interaction by id from the cache or database. It can filter by score and
/// database
///
/// # Returns
/// A `Vec` of `Interaction` objects corresponding to the given IDs. Ordered by score desc.
/// # Errors
/// Returns an error if the Interaction could not be loaded.
pub async fn load_interaction_by_target_a(
    ctx: &Context<'_>,
    id: String,
    score: Option<f64>,
    database: Option<InteractionSourceDatabase>,
) -> async_graphql::Result<Option<Vec<Interaction>>> {
    ctx.data_unchecked::<DataLoader<InteractionLoader>>()
        .load_one((id.clone(), score.map(f64::to_bits), database))
        .await
}

#[ComplexObject]
impl Interaction {
    async fn target_a(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Target>> {
        let target_id = &self.target_a;
        ctx.data_unchecked::<DataLoader<TargetLoader>>()
            .load_one(target_id.clone())
            .await
    }
    async fn target_b(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Target>> {
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
