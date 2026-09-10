use core::range;
use std::{collections::HashMap, fmt::format};

use async_graphql::{
    ComplexObject, Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::biosample::{Biosample, load_biosample_by_id},
    query::{
        QueryExt,
        paginate::{Page, Paged},
    },
};

// ---- models ----
//
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct BaselineExpression {
    target_id: String,
    target_from_source_id: Option<String>,
    #[graphql(skip)]
    tissue_biosample_id: Option<String>,
    #[graphql(skip)]
    tissue_biosample_parent_id: Option<String>,
    tissue_biosample_from_source: Option<String>,
    #[graphql(skip)]
    celltype_biosample_id: Option<String>,
    #[graphql(skip)]
    celltype_biosample_parent_id: Option<String>,
    celltype_biosample_from_source: Option<String>,
    min: Option<f64>,
    q1: Option<f64>,
    median: Option<f64>,
    q3: Option<f64>,
    max: Option<f64>,
    distribution_score: f64,
    specificity_score: Option<f64>,
    datasource_id: String,
    datatype_id: String,
    unit: String,
    quality_controls: Vec<String>,
    pub total: u64,
}

#[derive(Debug, Clone, Deserialize, SimpleObject, Eq, PartialEq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct TestKey {
    id: String,
    page: String,
}

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct BaselineExpressionRow {
    id: TestKey,
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
        let baseQuery = "((WITH paged AS (
            SELECT *, COUNT() OVER() as total
            FROM platform2606.baseline_expression
            WHERE targetId IN (?)
            LIMIT ?, ?
        )
        SELECT
            (any(paged.targetId), ?) AS id,
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

        let queries: Vec<String> = keys.iter().map(|k| baseQuery.to_string()).collect();

        // let mut full_query = self.ch.query(&queries.join(" UNION ALL "));

        let full_query = keys.iter().fold(
            self.ch.query(&queries.join(" UNION ALL ")),
            |acc: clickhouse::query::Query, key| {
                acc.bind(&key.0)
                    .bind(&key.1.index * &key.1.size)
                    .bind(&key.1.size)
                    .bind(format!("{},{}", &key.1.index, &key.1.size))
            },
        );

        println!("full query: {}", full_query.sql_display());

        // for key in keys {
        //     full_query = full_query
        //         .bind(&key.0)
        //         .bind(&key.1.size)
        //         .bind(&key.1.index)
        //         .clone();
        // }

        let result = full_query.fetch_all::<BaselineExpressionRow>().await?;

        let result2 = result
            .iter()
            .map(|res| {
                let page_iter = res.id.page.split_once(",").unwrap_or_default();
                (
                    (
                        res.id.id.clone(),
                        (Page {
                            index: page_iter.0.to_string().parse::<usize>().unwrap(),
                            size: page_iter.1.to_string().parse::<usize>().unwrap(),
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
    id: &String,
    page: Page,
) -> async_graphql::Result<Vec<BaselineExpression>> {
    Ok(ctx
        .data_unchecked::<DataLoader<BaselineExpressionLoader>>()
        .load_one((id.clone(), page))
        .await?
        .unwrap_or_default())
}

#[ComplexObject]
impl BaselineExpression {
    /// Tissue biosample entity.
    async fn tissue_biosample(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for tissue biosample.")] page: Page,
    ) -> async_graphql::Result<Paged<Biosample>> {
        match self.tissue_biosample_id.as_ref() {
            Some(id) => Ok(load_biosample_by_id(&ctx, &id)
                .await?
                .query()
                .paginate(page)),
            None => Ok(Paged {
                total: 0,
                items: Vec::new(),
            }),
        }
    }
    /// Tissue biosample entity.
    async fn tissue_biosample_parent(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for tissue biosample parent.")] page: Page,
    ) -> async_graphql::Result<Paged<Biosample>> {
        match self.tissue_biosample_parent_id.as_ref() {
            Some(id) => Ok(load_biosample_by_id(&ctx, &id)
                .await?
                .query()
                .paginate(page)),
            None => Ok(Paged {
                total: 0,
                items: Vec::new(),
            }),
        }
    }
    /// Cell type biosample entity.
    async fn celltype_biosample(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for celltype biosample.")] page: Page,
    ) -> async_graphql::Result<Paged<Biosample>> {
        match self.celltype_biosample_id.as_ref() {
            Some(id) => Ok(load_biosample_by_id(&ctx, &id)
                .await?
                .query()
                .paginate(page)),
            None => Ok(Paged {
                total: 0,
                items: Vec::new(),
            }),
        }
    }
    /// Cell type biosample parent entity.
    async fn celltype_biosample_parent(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for celltype biosample parent.")] page: Page,
    ) -> async_graphql::Result<Paged<Biosample>> {
        match self.celltype_biosample_parent_id.as_ref() {
            Some(id) => Ok(load_biosample_by_id(&ctx, &id)
                .await?
                .query()
                .paginate(page)),
            None => Ok(Paged {
                total: 0,
                items: Vec::new(),
            }),
        }
    }
}
