use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

use async_graphql::{
    ComplexObject, Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{biosample, enhancer_to_gene, target, variant::Chromosome},
    query::paginate::Paged,
};

///Score from a specific datasource.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ResourceScore {
    /// Name of the resource providing the score.
    name: String,
    /// Score value from the resource.
    value: f64,
}

///Regulatory enhancer/promoter regions to gene (target) predictions for
///a specific tissue/cell type based on the integration of experimental sources.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct EnhancerToGene {
    /// Chromosome containing the regulatory region.
    chromosome: Chromosome,
    /// Genomic start position of the regulatory region.
    start: u32,
    /// Genomic end position of the regulatory region.
    end: u32,
    // foreign keys
    #[graphql(skip)]
    gene_id: String,
    /// Name of the biosample where the interval was identified.
    biosample_name: String,
    #[graphql(skip)]
    biosample_id: String,
    /// Identifier of the biosample defined by the datasource provider.
    biosample_from_source_id: Option<String>,
    /// Type of regulatory region (e.g., enhancer, promoter).
    interval_type: String,
    /// Distance from the regulatory region to the transcription start site.
    distance_to_tss: i32,
    /// Combined score for the enhancer/promoter region to gene prediction.
    score: f64,
    /// Scores from individual resources used in prediction.
    resource_score: Vec<ResourceScore>,
    /// Identifier of the data source providing the regulatory region to gene prediction.
    datasource_id: String,
    /// PubMed identifier for the study providing the evidence [bioregistry:pubmed].
    pmid: String,
    /// Identifier of the study providing the experimental data.
    study_id: String,
    /// Quality control flags for this interval.
    quality_controls: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject, Eq, PartialEq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    chromosome: Chromosome,
    start: u32,
    end: u32,
    index: u32,
    size: u32,
}

impl Key {
    #[must_use]
    pub fn new(chromosome: Chromosome, start: u32, end: u32, index: u32, size: u32) -> Self {
        Self {
            chromosome,
            start,
            end,
            index,
            size,
        }
    }

    #[must_use]
    pub fn compute_hash(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.chromosome.hash(&mut h);
        self.start.hash(&mut h);
        self.end.hash(&mut h);
        self.index.hash(&mut h);
        self.size.hash(&mut h);
        h.finish()
    }
}

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EnhancerToGeneRow {
    key: usize,
    enhancer_to_genes: Paged<EnhancerToGene>,
}

// ---- loaders ----

#[derive(From)]
pub struct EnhancerToGeneLoader {
    ch: ClickHouse,
}

impl EnhancerToGeneLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<Key> for EnhancerToGeneLoader {
    type Value = Paged<EnhancerToGene>;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[Key]) -> Result<HashMap<Key, Self::Value>, Self::Error> {
        let base_query = "
            WITH
                ? AS q_chromosome,
                ? AS q_start,
                ? AS q_end,
                ? AS q_offset,
                ? AS q_limit,
                CAST(? AS UInt64) AS query_id,
                filtered AS
                (
                    SELECT *
                    FROM enhancer_to_gene
                    WHERE chromosome = q_chromosome
                      AND start <= q_start
                      AND end >= q_end
                ),
                (
                    SELECT count()
                    FROM filtered
                ) as count,
                paged AS
                (
                    SELECT *
                    FROM filtered
                    ORDER BY start, end
                    LIMIT q_limit OFFSET q_offset
                )
            SELECT
                query_id,
                tuple(
                        CAST(count AS UInt64),
                        groupArray(tuple(*)) as rows
                    )
            FROM paged
            GROUP BY query_id
            ";
        let queries: Vec<String> = keys.iter().map(|_| base_query.to_string()).collect();
        let full_query = keys.iter().enumerate().fold(
            self.ch.query(&queries.join(" UNION ALL ")),
            |acc: clickhouse::query::Query, (i, key)| {
                acc.bind(key.chromosome)
                    .bind(key.start)
                    .bind(key.end)
                    .bind(key.index * key.size)
                    .bind(key.size)
                    .bind(i as u64)
            },
        );
        let result = full_query.fetch_all::<EnhancerToGeneRow>().await?;
        let results_with_keys = result
            .iter()
            .map(|res| (keys[res.key].clone(), res.enhancer_to_genes.clone()))
            .collect();

        Ok(results_with_keys)
    }
}

/// Loads enhancer-to-gene associations for a given key.
///
/// # Returns
/// Returns a [`Paged`] result containing the enhancer-to-gene associations.
/// # Errors
/// Returns an error if the data cannot be loaded.
pub async fn load_enhancer_to_genes(
    ctx: &async_graphql::Context<'_>,
    key: enhancer_to_gene::Key,
) -> async_graphql::Result<Paged<EnhancerToGene>> {
    Ok(ctx
        .data_unchecked::<DataLoader<EnhancerToGeneLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}

#[ComplexObject]
impl EnhancerToGene {
    ///Cell type or tissue where the regulatory region to gene prediction was identified.
    async fn biosample(
        &self,
        ctx: &async_graphql::Context<'_>,
    ) -> async_graphql::Result<Option<biosample::Biosample>> {
        biosample::load_biosample_by_id(ctx, self.biosample_id.clone()).await
    }
    /// Predicted gene (target).
    async fn target(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<target::Target>> {
        target::load_target(ctx, self.gene_id.clone()).await
    }
}
