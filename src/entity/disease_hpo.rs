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
        hpo::{Hpo, HpoLoader, load_hpos},
    },
    query::Entity,
};

// ---- models ----

// Note: we cannot reorder this one as it is a nested tuple and those are deserialized in order
/// A container for all evidence-related attributes supporting the disease-phenotype association.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct PhenotypeEvidence {
    /// The category of biological information being provided (e.g., clinical, genetic, etc.).
    aspect: Option<String>,
    /// Indicates whether the evidence has been manually curated.
    bio_curation: Option<String>,
    /// Unique identifier for the disease from the original source.
    disease_from_source_id: String,
    /// The disease name as recorded in the original source.
    disease_from_source: String,
    /// Standardised disease name.
    disease_name: String,
    /// The type of evidence supporting the disease-phenotype relationship.
    evidence_type: Option<String>,
    /// The observed frequency of the phenotype in individuals with the disease.
    frequency: Option<String>,
    #[graphql(skip)]
    modifiers: Vec<String>,
    #[graphql(skip)]
    onset: Vec<String>,
    /// Specifies if a phenotype is not observed in a given disease.
    qualifier_not: bool,
    /// References or citations supporting the evidence [bioregistry:pubmed].
    references: Vec<String>,
    /// Indicates if the association is specific to a biological sex.
    sex: Option<String>,
    /// The data source from which the evidence was obtained.
    resource: String,
}

/// Clinical signs and symptoms observed in diseases or phenotypes. Signs and symptoms are
/// integrated from multiple sources including EFO, MONDO and HPO.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct DiseasePhenotype {
    /// Disease identifier.
    #[graphql(skip)]
    #[allow(dead_code)]
    disease: String,
    /// The phenotype linked to the disease.
    #[graphql(skip)]
    phenotype: String,
    /// A container for all evidence-related attributes supporting the disease-phenotype
    /// association.
    evidence: Vec<PhenotypeEvidence>,
}

/// Disease and phenotypes annotations.
#[derive(Debug, Row, Deserialize)]
struct DiseaseHpo {
    /// Disease identifier.
    disease: String,
    /// List of phenotypes associated with the disease.
    phenotypes: Vec<DiseasePhenotype>,
}

impl Entity for DiseasePhenotype {
    fn id(&self) -> &str { &self.phenotype } // unique within one disease's list
}

// ---- loaders ----

pub struct DiseasePhenotypeLoader {
    ch: ClickHouse,
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
        let rows: Vec<DiseaseHpo> = self
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

// ---- resolvers ----

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

fn fix_ids(ids: &[String]) -> Vec<String> { ids.iter().map(|id| id.replace(':', "_")).collect() }

#[ComplexObject]
impl PhenotypeEvidence {
    /// Additional characteristics or modifiers related to the phenotype.
    async fn modifiers(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Hpo>> {
        load_hpos(ctx, &fix_ids(&self.modifiers)).await
    }

    /// Age or stage of disease onset for the phenotype.
    async fn onset(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Hpo>> {
        load_hpos(ctx, &fix_ids(&self.onset)).await
    }
}
