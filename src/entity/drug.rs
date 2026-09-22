use std::{cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        clinical_indication::{ClinicalIndication, load_clinical_indications_from_drug},
        drug_warning::{DrugWarning, load_drug_warnings},
        publication::{
            LiteratureOcurrences, PublicationsArg, load_paged_publications_by_keyword_id_date,
        },
    },
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
        search::Searchable,
        sort::SortKey,
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
    /// Molecules corresponding to derivative compounds.
    #[graphql(skip)]
    child_chembl_ids: Vec<String>,
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

/// Contains the fields available for sorting drugs.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum)]
pub enum DrugSortField {
    Id,
    Name,
    DrugType,
    MaximumClinicalStage,
}

impl SortKey<Drug> for DrugSortField {
    fn compare(&self, a: &Drug, b: &Drug) -> Ordering {
        match self {
            Self::Id => a.id.cmp(&b.id),
            Self::Name => a.name.cmp(&b.name),
            Self::DrugType => a.drug_type.cmp(&b.drug_type),
            Self::MaximumClinicalStage => a.maximum_clinical_stage.cmp(&b.maximum_clinical_stage),
        }
    }
}

impl Searchable for Drug {
    fn matches_search(&self, needle: &str) -> bool {
        self.id.to_lowercase().contains(needle)
            || self.name.to_lowercase().contains(needle)
            || self
                .description
                .as_deref()
                .is_some_and(|d| d.to_lowercase().contains(needle))
            || self
                .synonyms
                .iter()
                .any(|s| s.label.to_lowercase().contains(needle))
    }
}

// ---- loaders ----

pub type DrugCache = Cache<String, Option<Drug>>;
static DRUG_CACHE: LazyLock<DrugCache> = LazyLock::new(entity_cache);

#[derive(From)]
pub struct DrugLoader {
    ch: ClickHouse,
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
pub async fn load_drug(ctx: &Context<'_>, id: String) -> async_graphql::Result<Option<Drug>> {
    ctx.data_unchecked::<DataLoader<DrugLoader>>()
        .load_one(id)
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
        #[graphql(desc = "List of Chembl IDs of the drugs to fetch.")] chembl_ids: Vec<String>,
        #[graphql(default, desc = "Pagination for the Drugs.")] page: Page,
    ) -> async_graphql::Result<Paged<Drug>> {
        let drugs = load_drugs(ctx, &chembl_ids).await?;
        Ok(drugs.query().paginate(page))
    }

    /// Retrieve a drug or a clinical candidate by an identifier.
    async fn drug(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Chembl ID of the drug to fetch.")] chembl_id: String,
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
                return load_drug(ctx, pid.clone()).await;
            }
            None => Ok(None),
        }
    }

    /// List of molecules corresponding to derivative compounds.
    async fn child_molecules(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Drug>> {
        load_drugs(ctx, &self.child_chembl_ids).await
    }

    /// Warnings present on drug as identified by ChEMBL.
    async fn drug_warnings(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for the drug warnings.")] page: Page,
    ) -> async_graphql::Result<Paged<DrugWarning>> {
        let items = load_drug_warnings(ctx, self.id.clone()).await?;
        Ok(items.query().paginate(page))
    }

    /// Clinical indications for this drug as reported by clinical trial records.
    async fn indications(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for the clinical indications.")] page: Page,
    ) -> async_graphql::Result<Paged<ClinicalIndication>> {
        let items = load_clinical_indications_from_drug(ctx, self.id.clone()).await?;
        Ok(items.query().paginate(page))
    }

    /// Return the list of publications that mention the main entity, alone or in combination with
    /// other entities
    async fn literature_ocurrences(
        &self,
        ctx: &Context<'_>,
        additional_ids: Option<Vec<String>>,
        start_year: Option<u32>,
        start_month: Option<u32>,
        end_year: Option<u32>,
        end_month: Option<u32>,
        #[graphql(desc = "Pagination for the interactions.")] page: Page,
    ) -> async_graphql::Result<LiteratureOcurrences> {
        let mut ids = additional_ids.unwrap_or_default().clone();
        ids.push(self.id.clone());

        let result = load_paged_publications_by_keyword_id_date(
            ctx,
            PublicationsArg {
                ids,
                start_year,
                start_month,
                end_year,
                end_month,
                page,
            },
        )
        .await?;

        let paged_result = match result {
            Some(publ) => LiteratureOcurrences {
                earliest_pub_year: publ.earliest_pub_year,
                total_count: publ.count,
                publications: Paged {
                    count: publ.filtered_count,
                    rows: publ.rows,
                },
            },
            None => LiteratureOcurrences::default(),
        };

        Ok(paged_result)
    }
}
