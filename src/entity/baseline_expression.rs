use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::biosample::{Biosample, load_biosample_by_id},
    query::paginate::Page,
};

// ---- models ----

/// Aggregated expression data generated from RNA-seq and mass spectrometry proteomic data.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct BaselineExpression {
    /// Unique identifier for the target.
    target_id: String,
    /// Target identifier from the source.
    target_from_source_id: Option<String>,
    #[graphql(skip)]
    tissue_biosample_id: Option<String>,
    #[graphql(skip)]
    tissue_biosample_parent_id: Option<String>,
    /// Cell type reported by study.
    tissue_biosample_from_source: Option<String>,
    #[graphql(skip)]
    celltype_biosample_id: Option<String>,
    #[graphql(skip)]
    celltype_biosample_parent_id: Option<String>,
    /// Tissue or cell type reported by study.
    celltype_biosample_from_source: Option<String>,
    /// Minimum value of expression.
    min: Option<f64>,
    /// First quantile of expression.
    q1: Option<f64>,
    /// Median value of expression.
    median: Option<f64>,
    /// Third quantile of expression.
    q3: Option<f64>,
    /// Maximum value of expression.
    max: Option<f64>,
    /// Proportion of biosamples, within the dataset, that display median expression above a
    /// threshold.
    distribution_score: f64,
    /// Measure of how specific expression is to a biosample.
    specificity_score: Option<f64>,
    /// Origin of expression data.
    datasource_id: String,
    /// Type of expression data.
    datatype_id: String,
    /// Unit for the target expression.
    unit: String,
    /// Quality control flags or notes for baseline expression.
    quality_controls: Vec<String>,
    #[graphql(skip)]
    pub total: u64,
}

impl BaselineExpression {
    /// Returns the total count of baseline expressions.
    #[must_use]
    pub fn total(&self) -> u64 { self.total }
}

#[derive(Debug, Clone, Deserialize, SimpleObject, Eq, PartialEq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct TestKey {
    target_id: String,
    index: usize,
    size: usize,
}

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct BaselineExpressionRow {
    key: TestKey,
    baseline_expressions: Vec<BaselineExpression>,
}

// ---- loaders ----

pub struct BaselineExpressionLoader {
    ch: ClickHouse,
}

impl BaselineExpressionLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<(String, Page)> for BaselineExpressionLoader {
    type Value = Vec<BaselineExpression>;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[(String, Page)],
    ) -> Result<HashMap<(String, Page), Self::Value>, Self::Error> {
        let base_query = "((WITH paged AS (
            SELECT *, COUNT() OVER() as total
            FROM baseline_expression
            WHERE targetId IN (?)
            LIMIT ?, ?
        )
        SELECT
            any(paged.targetId) as targetId, CAST(? AS UInt64) as size, CAST(? AS UInt64) as limit,
            groupArray((
                paged.targetId,
                paged.targetFromSourceId,
                paged.tissueBiosampleId,
                paged.tissueBiosampleParentId,
                paged.tissueBiosampleFromSource,
                paged.celltypeBiosampleId,
                paged.celltypeBiosampleParentId,
                paged.celltypeBiosampleFromSource,
                paged.min, paged.q1, paged.median, paged.q3, paged.max,
                paged.distribution_score,
                paged.specificity_score,
                paged.datasourceId, paged.datatypeId, paged.unit,
                paged.qualityControls,
                paged.total
            )) AS baselineExpressions
        FROM paged
        GROUP BY paged.targetId))";

        let queries: Vec<String> = keys.iter().map(|_| base_query.to_string()).collect();

        let full_query = keys.iter().fold(
            self.ch.query(&queries.join(" UNION ALL ")),
            |acc: clickhouse::query::Query, key| {
                acc.bind(&key.0)
                    .bind(key.1.index * key.1.size)
                    .bind(key.1.size)
                    .bind(key.1.index)
                    .bind(key.1.size)
            },
        );

        let result = full_query.fetch_all::<BaselineExpressionRow>().await?;
        let result2 = result
            .iter()
            .map(|res| {
                (
                    (
                        res.key.target_id.clone(),
                        (Page {
                            index: res.key.index,
                            size: res.key.size,
                        }),
                    ),
                    res.baseline_expressions.clone(),
                )
            })
            .collect();

        Ok(result2)
    }
}

/// Loads Baseline Expressions by the target id from the cache or database.
///
/// # Returns
/// A `Vec` of `BaselineExpression` objects corresponding to the given target IDs.
/// # Errors
/// Returns an error if the Baseline Expression could not be loaded.
pub async fn load_baseline_expression_by_target(
    ctx: &Context<'_>,
    id: String,
    page: Page,
) -> async_graphql::Result<Vec<BaselineExpression>> {
    Ok(ctx
        .data_unchecked::<DataLoader<BaselineExpressionLoader>>()
        .load_one((id, page))
        .await?
        .unwrap_or_default())
}

#[ComplexObject]
impl BaselineExpression {
    /// Tissue biosample reported by study.
    async fn tissue_biosample(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<Biosample>> {
        match self.tissue_biosample_id.as_ref() {
            Some(id) => Ok(load_biosample_by_id(ctx, id).await?),
            None => Ok(None),
        }
    }
    /// Tissue biosample parent reported by study.
    async fn tissue_biosample_parent(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<Biosample>> {
        match self.tissue_biosample_parent_id.as_ref() {
            Some(id) => Ok(load_biosample_by_id(ctx, id).await?),
            None => Ok(None),
        }
    }
    /// Cell type biosample reported by study.
    async fn celltype_biosample(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<Biosample>> {
        match self.celltype_biosample_id.as_ref() {
            Some(id) => Ok(load_biosample_by_id(ctx, id).await?),
            None => Ok(None),
        }
    }
    /// Cell type biosample parent reported by study.
    async fn celltype_biosample_parent(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<Biosample>> {
        match self.celltype_biosample_parent_id.as_ref() {
            Some(id) => Ok(load_biosample_by_id(ctx, id).await?),
            None => Ok(None),
        }
    }
}
