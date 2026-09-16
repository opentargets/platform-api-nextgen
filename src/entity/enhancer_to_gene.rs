use std::{collections::HashMap, hash::{Hash, Hasher}, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, InputType, Object, SimpleObject, dataloader::{DataLoader, Loader}
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;
use serde_repr::Deserialize_repr;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{enhancer_to_gene, variant::Chromosome},
    query::{
        QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
    },
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
// #[graphql(complex)]
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
    #[graphql(skip)]
    study_id: String,
    /// Quality control flags for this interval.
    quality_controls: Vec<String>,
    // meta
    #[graphql(skip)]
    total: u64,
}


impl EnhancerToGene {
    /// Returns the total count of enhancer-to-gene predictions.
    #[must_use]
    pub fn total(&self) -> u64 { self.total }
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
    pub fn new(chromosome: Chromosome, start: u32, end: u32, index: u32, size: u32) -> Self {
        Self {
            chromosome,
            start,
            end,
            index,
            size,
        }
    }
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
    key: u64,
    enhancer_to_genes: Vec<EnhancerToGene>,
}


// ---- loaders ----

pub struct EnhancerToGeneLoader {
    ch: ClickHouse,
}

impl EnhancerToGeneLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<Key> for EnhancerToGeneLoader {
    type Value = Vec<EnhancerToGene>;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[Key],
    ) -> Result<HashMap<Key, Self::Value>, Self::Error> {
        let base_query = "
            WITH
            ? as q_chromosome,
            ? as q_start,
            ? as q_end,
            ? as q_offset,
            ? as q_size,
            CAST(? AS UInt64) as query_id,
            paged AS (
                SELECT *, COUNT() OVER() AS total
                FROM enhancer_to_gene
                WHERE
                    chromosome = q_chromosome AND start <= q_start AND end >= q_end
                LIMIT q_offset, q_size
            )
            SELECT
                query_id,
                groupArray(
                    tuple(*)
                ) AS enhancer_to_genes
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
            .map(|res| {
                (
                    keys[res.key as usize].clone(),
                    res.enhancer_to_genes.clone(),
                )
            })
            .collect();

        Ok(results_with_keys)
    }
}

pub async fn load_enhancer_to_genes(
    ctx: &async_graphql::Context<'_>,
    key: enhancer_to_gene::Key,
    page: Page,
) -> async_graphql::Result<Vec<EnhancerToGene>> {
    Ok(ctx
        .data_unchecked::<DataLoader<EnhancerToGeneLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
