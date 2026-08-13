use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        disease::{Disease, DiseaseLoader},
        hpo::{Hpo, HpoLoader},
    },
    query::Entity,
};

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct PhenotypeEvidence {
    aspect: Option<String>,
    bio_curation: Option<String>,
    disease_from_source_id: String,
    disease_from_source: String,
    disease_name: String,
    evidence_type: Option<String>,
    frequency: Option<String>,
    modifiers: Vec<String>,
    onset: Vec<String>,
    qualifier_not: bool,
    references: Vec<String>,
    sex: Option<String>,
    resource: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct DiseasePhenotype {
    #[graphql(skip)]
    #[allow(dead_code)]
    disease: String,
    #[graphql(skip)]
    phenotype: String,
    evidence: Vec<PhenotypeEvidence>,
}

#[ComplexObject]
impl DiseasePhenotype {
    /// Resolved HPO term. Null when the phenotype id is not an HPO (e.g. MONDO).
    #[graphql(name = "phenotypeHPO")]
    async fn phenotype_hpo(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Hpo>> {
        ctx.data_unchecked::<DataLoader<HpoLoader>>()
            .load_one(self.phenotype.clone())
            .await
    }

    /// Resolved EFO term. Null when the phenotype id is an HPO.
    #[graphql(name = "phenotypeEFO")]
    async fn phenotype_efo(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Disease>> {
        ctx.data_unchecked::<DataLoader<DiseaseLoader>>()
            .load_one(self.phenotype.clone())
            .await
    }
}

impl Entity for DiseasePhenotype {
    fn id(&self) -> &str { &self.phenotype } // unique within one disease's list
}

#[derive(Debug, Row, Deserialize)]
struct DiseaseHpoRow {
    disease: String,
    phenotypes: Vec<DiseasePhenotype>,
}

pub struct DiseasePhenotypeLoader {
    pub ch: ClickHouse,
}

impl DiseasePhenotypeLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<String> for DiseasePhenotypeLoader {
    type Value = Vec<DiseasePhenotype>;
    type Error = async_graphql::Error;
    async fn load(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Vec<DiseasePhenotype>>, Self::Error> {
        let rows: Vec<DiseaseHpoRow> = self
            .ch
            .query("SELECT ?fields FROM disease_hpo WHERE disease IN ?")
            .bind(ids)
            .fetch_all()
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.disease, r.phenotypes))
            .collect())
    }
}
