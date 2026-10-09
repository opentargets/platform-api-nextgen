use std::collections::HashMap;

use async_graphql::{
    Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{datasource::clickhouse::ClickHouse, entity::clinical_report::ClinicalDiseaseListItem};

// ---- models ----

/// Target-drug associations derived from clinical reports, capturing the maximum clinical stage and
/// associated diseases for each target-drug pair.
#[derive(Debug, Clone, Deserialize, SimpleObject, Row)]
#[serde(rename_all = "camelCase")]
#[graphql(name = "ClinicalTargetFromTarget")]
pub struct ClinicalTarget {
    id: String,
    #[graphql(skip)]
    drug_id: String,
    #[graphql(skip)]
    target_id: String,
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
        Ok(group_by(rows, |row| &row.target_id))
    }
}

fn group_by(
    rows: Vec<ClinicalTarget>,
    key: fn(&ClinicalTarget) -> &String,
) -> HashMap<String, Vec<ClinicalTarget>> {
    rows.into_iter().fold(HashMap::new(), |mut acc, row| {
        acc.entry(key(&row).clone()).or_default().push(row);
        acc
    })
}

/// Loads the clinical target recorded for a single target by its target id.
///
/// # Returns
/// The target's clinical target, or None if the target has none.
/// # Errors
/// Returns an error if the clinical target could not be loaded.
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
