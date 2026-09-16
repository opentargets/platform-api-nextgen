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
        disease::{Disease, load_disease},
        drug::{Drug, load_drug},
    },
};

// ---- models ----

/// A drug-disease pair as reported by clinical trial records.
#[derive(Debug, Clone, Deserialize, Row, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct ClinicalIndication {
    /// Hash of drugId and diseaseId.
    id: String,
    #[graphql(skip)]
    drug_id: String,
    #[graphql(skip)]
    disease_id: String,
    /// Maximum Clinical Development Status for the association.
    max_clinical_stage: String,
    clinical_report_ids: Vec<String>,
}

// ---- loaders ----

#[derive(From)]
pub struct ClinicalIndicationFromDrugLoader {
    ch: ClickHouse,
}

impl Loader<String> for ClinicalIndicationFromDrugLoader {
    type Value = Vec<ClinicalIndication>;
    type Error = async_graphql::Error;

    #[tracing::instrument(skip_all, level = "debug", fields(n = ids.len()))]
    async fn load(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Vec<ClinicalIndication>>, Self::Error> {
        let rows = self
            .ch
            .query("SELECT ?fields FROM clinical_indication_drug WHERE drugId IN ?")
            .bind(ids)
            .fetch_all()
            .await?;
        Ok(group_by(rows, |row| &row.drug_id))
    }
}

#[derive(From)]
pub struct ClinicalIndicationFromDiseaseLoader {
    ch: ClickHouse,
}

impl Loader<String> for ClinicalIndicationFromDiseaseLoader {
    type Value = Vec<ClinicalIndication>;
    type Error = async_graphql::Error;

    #[tracing::instrument(skip_all, level = "debug", fields(n = ids.len()))]
    async fn load(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Vec<ClinicalIndication>>, Self::Error> {
        let rows = self
            .ch
            .query("SELECT ?fields FROM clinical_indication_disease WHERE diseaseId IN ?")
            .bind(ids)
            .fetch_all()
            .await?;
        Ok(group_by(rows, |row| &row.disease_id))
    }
}

/// Groups the rows by the given key. Both tables hold one row per drug-disease pair, while a
/// loader returns one entry per key, so the rows are collected into a list per key.
fn group_by(
    rows: Vec<ClinicalIndication>,
    key: fn(&ClinicalIndication) -> &String,
) -> HashMap<String, Vec<ClinicalIndication>> {
    rows.into_iter().fold(HashMap::new(), |mut acc, row| {
        acc.entry(key(&row).clone()).or_default().push(row);
        acc
    })
}

/// Loads the clinical indications recorded for a single drug by its ChEMBL id.
///
/// # Returns
/// The drug's clinical indications, or an empty `Vec` if it has none.
/// # Errors
/// Returns an error if the clinical indications could not be loaded.
pub async fn load_clinical_indications_from_drug(
    ctx: &Context<'_>,
    chembl_id: String,
) -> async_graphql::Result<Vec<ClinicalIndication>> {
    Ok(ctx
        .data_unchecked::<DataLoader<ClinicalIndicationFromDrugLoader>>()
        .load_one(chembl_id)
        .await?
        .unwrap_or_default())
}

/// Loads the clinical indications recorded for a single disease by its EFO id.
///
/// # Returns
/// The disease's clinical indications, or an empty `Vec` if it has none.
/// # Errors
/// Returns an error if the clinical indications could not be loaded.
pub async fn load_clinical_indications_from_disease(
    ctx: &Context<'_>,
    efo_id: String,
) -> async_graphql::Result<Vec<ClinicalIndication>> {
    Ok(ctx
        .data_unchecked::<DataLoader<ClinicalIndicationFromDiseaseLoader>>()
        .load_one(efo_id)
        .await?
        .unwrap_or_default())
}

#[ComplexObject]
impl ClinicalIndication {
    /// The molecule entity related to this clinical indication.
    async fn drug(&self, ctx: &Context<'_>) -> async_graphql::Result<Drug> {
        load_drug(ctx, self.drug_id.clone())
            .await?
            .ok_or_else(|| async_graphql::Error::new("drug not found"))
    }

    /// The disease entity related to this clinical indication.
    async fn disease(&self, ctx: &Context<'_>) -> async_graphql::Result<Disease> {
        load_disease(ctx, self.disease_id.clone())
            .await?
            .ok_or_else(|| async_graphql::Error::new("disease not found"))
    }
}
