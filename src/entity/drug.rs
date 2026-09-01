use std::{collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
    },
};

// ---- models ----

/// Cross-reference information for a drug molecule.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
pub struct DrugReferences {
    /// Source database providing the cross-reference.
    source: String,
    /// List of identifiers from the source database.
    ids: Vec<String>,
}

/// Drug label with source information.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
pub struct DrugLabelAndSource {
    /// Label value (e.g., synonym, symbol).
    label: String,
    /// Source database of the label.
    source: String,
}

/// Core annotation for drug or clinical candidate molecules. A drug in the platform is understood
/// as any bioactive molecule with drug-like properties included in the EMBL-EBI ChEMBL database.
/// All ChEMBL molecules fullfilling any of the next criteria are included in the database: a)
/// Molecules with a known indication. b) Molecules with a known mechanism of action c) ChEMBL
/// molecules included in the DrugBank database d) Molecules that are acknowledged as chemical
/// probes.
#[derive(Debug, Clone, Deserialize, SimpleObject, Row)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Drug {
    /// Drug or clinical candidate molecule identifier.
    id: String,
    /// Generic name of the drug molecule.
    name: String,
    /// List of alternative names for the drug, each with its source (e.g. ChEMBL, or AACT for
    /// names mined from clinical trials).
    synonyms: Vec<DrugLabelAndSource>,
    /// List of brand names for the drug, each with its source.
    trade_names: Vec<DrugLabelAndSource>,
    /// Classification of the molecule's therapeutic category or chemical class (e.g. Antibody).
    #[allow(clippy::struct_field_names)]
    drug_type: String, // TODO: make it enum type because 11 drug types
    /// Cross-reference information for this molecule from external databases.
    cross_references: Vec<DrugReferences>,
    /// Parent molecule for derivative compounds.
    #[graphql(skip)]
    parent_id: Option<String>,
    /// Highest clinical stage reached by the drug or clinical candidate molecule.
    maximum_clinical_stage: String,
    /// Summary of the drug's clinical development.
    description: Option<String>,
    /// Mol Block is a chemical structure file format that serves as a connection table,
    /// representing molecules through a list of atoms, bonds, and spatial coordinates.
    molblock: Option<String>,
}

// ---- query utilities ----

impl Entity for Drug {
    fn id(&self) -> &str { &self.id }
}

// ---- loaders ----

pub type DrugCache = Cache<String, Option<Drug>>;
static DRUG_CACHE: LazyLock<DrugCache> = LazyLock::new(entity_cache);

pub struct DrugLoader {
    ch: ClickHouse,
}

impl DrugLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl CachedLoader for DrugLoader {
    type Key = String;
    type Value = Drug;

    fn cache(&self) -> &DrugCache { &DRUG_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM drug WHERE id IN ?")
            .bind(misses)
            .fetch_all::<Drug>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for DrugLoader {
    type Value = Drug;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[String]) -> Result<HashMap<String, Drug>, async_graphql::Error> {
        self.load_cached(keys).await
    }
}

#[allow(clippy::missing_errors_doc)]
pub async fn load_drugs(ctx: &Context<'_>, ids: &[String]) -> async_graphql::Result<Vec<Drug>> {
    load_ordered(ctx.data_unchecked::<DataLoader<DrugLoader>>(), ids).await
}

#[allow(clippy::missing_errors_doc)]
pub async fn load_drug(ctx: &Context<'_>, id: &str) -> async_graphql::Result<Option<Drug>> {
    ctx.data_unchecked::<DataLoader<DrugLoader>>()
        .load_one(id.to_string())
        .await
}

// ---- resolvers ----

#[derive(Default)]
pub struct DrugQuery;

#[Object]
impl DrugQuery {
    /// Retrieve multiple drugs or clinical candidates by identifiers.
    async fn drugs(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "List of Chembl IDs.")] chembl_ids: Vec<String>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<Drug>> {
        let drugs = load_drugs(ctx, &chembl_ids).await?;
        Ok(drugs.query().paginate(page))
    }

    /// Retrieve a drug or a clinical candidate by an identifier.
    async fn drug(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Chembl ID.")] chembl_id: String,
    ) -> async_graphql::Result<Option<Drug>> {
        ctx.data_unchecked::<DataLoader<DrugLoader>>()
            .load_one(chembl_id)
            .await
    }
}

#[ComplexObject]
impl Drug {
    /// Parent molecule for derivative compounds.
    async fn parent_molecule(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Drug>> {
        match &self.parent_id {
            Some(pid) => {
                return load_drug(ctx, pid).await;
            }
            None => Ok(None),
        }
    }
}
