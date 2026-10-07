use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        clinical_report::{ClinicalDiseaseListItem, ClinicalReport, load_clinical_reports},
        drug::{Drug, load_drug},
    },
};

// ---- models ----

/// Target-drug associations derived from clinical reports, capturing the maximum clinical stage and
/// associated diseases for each target-drug pair.
#[derive(Debug, Clone, Deserialize, SimpleObject, Row)]
#[serde(rename_all = "camelCase")]
#[graphql(complex, name = "ClinicalTargetFromTarget")]
pub struct ClinicalTarget {
    /// Hash between drugId and targetId.
    id: String,
    #[graphql(skip)]
    drug_id: String,
    #[graphql(skip)]
    target_id: String,
    /// Diseases associated with the clinical reports linked to the drug in question.
    diseases: Vec<ClinicalDiseaseListItem>,
    max_clinical_stage: String,
    #[graphql(skip)]
    clinical_report_ids: Vec<String>,
}

// ---- loaders ----

#[derive(From)]
pub struct ClinicalTargetLoader {
    ch: ClickHouse,
}

impl Loader<String> for ClinicalTargetLoader {
    type Value = Vec<ClinicalTarget>;
    type Error = async_graphql::Error;

    #[tracing::instrument(skip_all, level = "debug", fields(n = ids.len()))]
    async fn load(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Vec<ClinicalTarget>>, Self::Error> {
        let rows = self
            .ch
            .query("SELECT ?fields FROM clinical_target WHERE targetId IN ?")
            .bind(ids)
            .fetch_all()
            .await?;
        Ok(rows
            .into_iter()
            .fold(HashMap::new(), |mut acc, row: ClinicalTarget| {
                acc.entry(row.target_id.clone()).or_default().push(row);
                acc
            }))
    }
}

/// Loads the clinical targets for a target by its Ensembl id.
///
/// # Returns
/// The target's clinical targets, or an empty `Vec` if it has none.
/// # Errors
/// Returns an error if the clinical targets could not be loaded.
pub async fn load_clinical_targets_from_target(
    ctx: &Context<'_>,
    ensembl_id: String,
) -> async_graphql::Result<Vec<ClinicalTarget>> {
    Ok(ctx
        .data_unchecked::<DataLoader<ClinicalTargetLoader>>()
        .load_one(ensembl_id)
        .await?
        .unwrap_or_default())
}

#[ComplexObject]
impl ClinicalTarget {
    /// Drug or clinical candidate entity.
    async fn drug(&self, ctx: &Context<'_>) -> async_graphql::Result<Drug> {
        load_drug(ctx, self.drug_id.clone())
            .await?
            .ok_or_else(|| async_graphql::Error::new("drug not found"))
    }

    /// Reports related with the drug in question.
    async fn clinical_reports(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<ClinicalReport>> {
        load_clinical_reports(ctx, &self.clinical_report_ids).await
    }
}
