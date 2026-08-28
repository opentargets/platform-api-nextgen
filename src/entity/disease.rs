use std::{cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        association::{
            AssocArgs, AssociationSort, DatasourcePolicyInput, TargetAssociation, load_associations,
        },
        disease_hpo::{DiseasePhenotype, DiseasePhenotypeLoader},
        target::Target,
    },
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
        search::Searchable,
        sort::{Sort, SortKey},
    },
};

// ---- models ----

/// List of synonymous disease labels.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DiseaseSynonym {
    // Whether this is an exact, related, broad or narrow synonym.
    relation: String,
    // The terms that are synonymous with the disease.
    terms: Vec<Option<String>>,
}

/// Core annotation for diseases or phenotypes. A disease or phenotype in the Platform is understood
/// as any disease, phenotype, biological process or measurement that might have any type of
/// causality relationship with a human target. The EMBL-EBI Experimental Factor Ontology (EFO)
/// (slim version) is used as scaffold for the disease or phenotype entity.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Disease {
    // Identity
    /// Open Targets disease identifier [bioregistry:efo].
    id: String,
    /// Name of the disease.
    name: String,
    /// Description of the disease.
    description: Option<String>,

    // Ontology
    /// Boolean column indicating if a disease is root of the ontology tree, a therapeutic area.
    is_therapeutic_area: bool,
    /// List of major therapeutic areas the disease belongs to.
    therapeutic_areas: Vec<String>,
    #[graphql(skip)]
    parents: Vec<String>,
    #[graphql(skip)]
    children: Vec<String>,
    /// List of all ancestral disease terms.
    ancestors: Vec<String>,
    /// List of all descendant terms.
    descendants: Vec<String>,

    // Crossreferences
    /// List of synonyms for the disease.
    synonyms: Vec<DiseaseSynonym>,
    /// List of obsoleted terms.
    obsolete_terms: Vec<String>,
    /// Cross-references in other disease ontologies.
    db_x_refs: Vec<String>,

    // Location
    /// EFO terms for direct anatomical locations.
    direct_location_ids: Vec<String>,
    /// EFO terms for indirect anatomical locations (propagated).
    indirect_location_ids: Vec<String>,

    // Studies
    /// List of studies associated with the disease.
    study_ids: Vec<String>,
    /// List of studies associated with the disease and its descendants.
    indirect_study_ids: Vec<String>,
}

impl Disease {
    /// Returns the study IDs associated with the disease.
    ///
    /// If `indirect` is `true`, the study IDs will include indirect study IDs.
    #[must_use]
    pub fn into_study_ids(mut self, indirect: bool) -> Vec<String> {
        if indirect {
            self.study_ids.append(&mut self.indirect_study_ids);
        }
        self.study_ids
    }
}

// ---- query utilities ----

impl Entity for Disease {
    fn id(&self) -> &str { &self.id }
}

/// Contains the fields available for sorting diseases.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum)]
pub enum DiseaseSortField {
    Id,
    Name,
    IsTherapeuticArea,
}

impl SortKey<Disease> for DiseaseSortField {
    fn compare(&self, a: &Disease, b: &Disease) -> Ordering {
        match self {
            Self::Id => a.id.cmp(&b.id),
            Self::Name => a.name.cmp(&b.name),
            Self::IsTherapeuticArea => a.is_therapeutic_area.cmp(&b.is_therapeutic_area),
        }
    }
}

impl Searchable for Disease {
    fn matches_search(&self, needle: &str) -> bool {
        self.id.to_lowercase().contains(needle)
            || self.name.to_lowercase().contains(needle)
            || self
                .description
                .as_deref()
                .is_some_and(|d| d.to_lowercase().contains(needle))
            || self.synonyms.iter().flat_map(|s| &s.terms).any(|t| {
                t.as_deref()
                    .is_some_and(|f| f.to_lowercase().contains(needle))
            })
    }
}

// ---- loaders ----

pub type DiseaseCache = Cache<String, Option<Disease>>;
static DISEASE_CACHE: LazyLock<DiseaseCache> = LazyLock::new(entity_cache);

pub struct DiseaseLoader {
    ch: ClickHouse,
}

impl DiseaseLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl CachedLoader for DiseaseLoader {
    type Key = String;
    type Value = Disease;

    fn cache(&self) -> &DiseaseCache { &DISEASE_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM disease WHERE id IN ?")
            .bind(misses)
            .fetch_all::<Disease>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for DiseaseLoader {
    type Value = Disease;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[String],
    ) -> Result<HashMap<String, Disease>, async_graphql::Error> {
        self.load_cached(keys).await
    }
}

/// Load diseases by their EFO IDs.
///
/// This function uses a [`DataLoader`] to fetch diseases from the cache or database.
///
/// # Returns
/// A [`Vec`] of [`Disease`] entities.
/// # Errors
/// Returns an [`async_graphql::Error`] if the database query fails.
pub async fn load_diseases(
    ctx: &Context<'_>,
    ids: &[String],
) -> async_graphql::Result<Vec<Disease>> {
    load_ordered(ctx.data_unchecked::<DataLoader<DiseaseLoader>>(), ids).await
}

/// Load a disease by its ID.
///
/// This function uses a [`DataLoader`] to fetch a disease from the cache or database.
///
/// # Returns
/// An [`Option`] of [`Disease`] entity.
/// # Errors
/// Returns an [`async_graphql::Error`] if the database query fails.
pub async fn load_disease(ctx: &Context<'_>, id: &str) -> async_graphql::Result<Option<Disease>> {
    ctx.data_unchecked::<DataLoader<DiseaseLoader>>()
        .load_one(id.to_string())
        .await
}

// ---- resolvers ----

#[derive(Default)]
pub struct DiseaseQuery;

#[Object]
impl DiseaseQuery {
    /// Fetch diseases by EFO ID.
    async fn diseases(
        &self,
        ctx: &Context<'_>,
        efo_ids: Vec<String>,
        search: Option<String>,
        sort: Option<Sort<DiseaseSortField>>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<Disease>> {
        let items = load_diseases(ctx, &efo_ids).await?;
        Ok(items
            .query()
            .search(search.as_deref())
            .sort(sort.as_ref())
            .paginate(page))
    }

    async fn disease(
        &self,
        ctx: &Context<'_>,
        efo_id: String,
    ) -> async_graphql::Result<Option<Disease>> {
        ctx.data_unchecked::<DataLoader<DiseaseLoader>>()
            .load_one(efo_id)
            .await
    }
}

#[ComplexObject]
impl Disease {
    /// Direct parent terms in the disease ontology.
    async fn parents(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Disease>> {
        load_diseases(ctx, &self.parents).await
    }

    /// Direct child terms in the disease ontology.
    async fn children(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Disease>> {
        load_diseases(ctx, &self.children).await
    }

    /// Clinical signs and symptoms observed in diseases or phenotypes. Signs and symptoms are
    /// integrated from multiple sources including EFO, MONDO and HPO.
    async fn phenotypes(
        &self,
        ctx: &Context<'_>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<DiseasePhenotype>> {
        let items = ctx
            .data_unchecked::<DataLoader<DiseasePhenotypeLoader>>()
            .load_one(self.id.clone())
            .await?
            .unwrap_or_default();
        Ok(items.query().paginate(page))
    }

    /// Disease-target associations computed on the fly with configurable datasource weights and
    /// filters.
    #[allow(clippy::unused_async)]
    async fn associated_targets(
        &self,
        ctx: &Context<'_>,

        #[graphql(
            name = "Bs",
            default,
            desc = "List of target ids to use as the second dimension for associations."
        )]
        bs: Vec<String>,

        #[graphql(name = "BFilter", desc = "Filter to apply to the B dimension items.")]
        b_filter: Option<String>,

        #[graphql(default, desc = "List of the facet ids to filter by (using AND).")]
        facet_filters: Vec<String>,

        #[graphql(
            default,
            desc = "Expand the association set indirectly: for a disease, include its ontology \
                    descendants."
        )]
        indirect: bool,

        #[graphql(
            default,
            desc = "List of datasource policies. If ommitted, use the default."
        )]
        datasources: Option<Vec<DatasourcePolicyInput>>,

        #[graphql(
            default,
            desc = "Ordering for the associations. Can either be `score` to use the overall \
                    association score (default), a datasource id (e.g., `impc`), or a datatype id \
                    (e.g., `animal_model`)."
        )]
        sort: AssociationSort,

        #[graphql(default, desc = "Pagination for the associations.")] page: Page,
    ) -> async_graphql::Result<Paged<TargetAssociation>> {
        let args = AssocArgs {
            bs,
            b_filter,
            facet_filters,
            indirect,
            include_measurements: false, // Only used in target to include measurements diseases
            datasources,
            sort,
            page,
        };
        load_associations::<Target>(ctx, &self.id, args)
    }
}
