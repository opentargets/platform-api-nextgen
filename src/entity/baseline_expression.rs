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
}

// ---- loaders ----
pub struct BaselineExpressionLoader {
    ch: ClickHouse,
}

impl BaselineExpressionLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<String> for BaselineExpressionLoader {
    type Value = Vec<BaselineExpression>;
    type Error = async_graphql::Error;

    async fn load(&self, key: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        let rows: Vec<BaselineExpression> = self
            .ch
            .query(
                "SELECT \
                    targetId, targetFromSourceId, \
                    tissueBiosampleId, tissueBiosampleParentId, tissueBiosampleFromSource, \
                    celltypeBiosampleId, celltypeBiosampleParentId, celltypeBiosampleFromSource, \
                    min, q1, median, q3, max, \
                    distribution_score, specificity_score, \
                    datasourceId, datatypeId, unit, qualityControls \
                 FROM platform2606.baseline_expression \
                 WHERE targetId IN ?",
            )
            .bind(key)
            .fetch_all()
            .await?;
        Ok(rows.into_iter().fold(
            key.iter().cloned().map(|k| (k, Vec::new())).collect(),
            |mut acc: HashMap<String, Vec<BaselineExpression>>, row| {
                acc.entry(row.target_id.clone()).or_default().push(row);
                acc
            },
        ))
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
) -> async_graphql::Result<Paged<BaselineExpression>> {
    let items = ctx
        .data_unchecked::<DataLoader<BaselineExpressionLoader>>()
        .load_one(id.clone())
        .await?
        .unwrap_or_default();
    Ok(items.query().paginate(page))
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
            Some(id) => load_biosample_by_id(&ctx, &id, page).await,
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
            Some(id) => load_biosample_by_id(&ctx, &id, page).await,
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
            Some(id) => load_biosample_by_id(&ctx, &id, page).await,
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
            Some(id) => load_biosample_by_id(&ctx, &id, page).await,
            None => Ok(Paged {
                total: 0,
                items: Vec::new(),
            }),
        }
    }
}
