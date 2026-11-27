use std::{cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use moka::future::Cache;
use serde::Deserialize;
use strum::EnumIter;

#[cfg(feature = "product-ppp")]
use crate::entity::otar_project_ppp::{OtarProject, load_otar_projects};
use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        association::{
            AssociationArguments, AssociationSort, DISEASE_INDIRECT_WEIGHT_DEFAULT, Datasource,
            DatasourcePolicyOverride, EntityWithAssociations, TargetAssociation, load_associations,
        },
        association_timeseries_ppp::{
            AggregationType, AssociationTimeseries, AssociationTimeseriesArguments, TimeseriesKey,
            load_association_timeseries,
        },
        clinical_indication::{self, ClinicalIndication, load_clinical_indications_from_disease},
        disease_hpo::{DiseasePhenotype, DiseasePhenotypeLoader},
        evidence::{Evidence, EvidenceKey, load_evidences},
        publication::{Publication, PublicationArguments, load_publications},
        similar_entity::{SimilarEntityArguments, Similarity, load_similar_entities},
        target::Target,
    },
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged, PagedWithStats, capped_page_complexity, page_complexity},
        search::{Search, Searchable, contains},
        sort::{Nulls, Sort, SortDirection, SortKey, Term, UniqueSortKeys},
    },
    view::{
        disease_studies as ds,
        evidence::{EvidenceScope, EvidenceViews},
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
    #[graphql(skip)]
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

impl EntityWithAssociations for Disease {
    const TABLE: &'static str = "associations_otf_disease";
    const B_TABLE: &'static str = "targets";
    const B_SEARCH: &'static str = "lower(concat(id, ' ', approvedSymbol, ' ', approvedName))";
    type B = Target;
    async fn a_ids(ch: &ClickHouse, anchor: &str) -> async_graphql::Result<Vec<String>> {
        Ok(ch
            .query("SELECT descendants FROM disease WHERE id = ?")
            .bind(anchor)
            .fetch_optional::<Vec<String>>()
            .await?
            .unwrap_or_default())
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

const DEFAULT_SORT: [Sort<DiseaseSortField>; 1] = [Sort {
    key: DiseaseSortField::Id,
    direction: SortDirection::Ascending,
}];

impl DiseaseSortField {
    /// By ID, ascending.
    fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }
}

impl SortKey<Disease> for DiseaseSortField {
    fn compare(&self, a: &Disease, b: &Disease, direction: SortDirection) -> Ordering {
        let primary = Term::new(direction, Nulls::Last);
        match self {
            Self::Id => primary.cmp(&a.id, &b.id),
            Self::Name => primary.cmp(&a.name, &b.name),
            Self::IsTherapeuticArea => primary.cmp(&a.is_therapeutic_area, &b.is_therapeutic_area),
        }
    }
}

/// Text fields that `search` can match against.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
pub enum DiseaseSearchField {
    Id,
    Name,
    Description,
    /// Any synonym, of any relation type.
    Synonyms,
}

impl Searchable for Disease {
    type Field = DiseaseSearchField;

    fn field_matches(&self, field: DiseaseSearchField, needle: &str) -> bool {
        use DiseaseSearchField as F;
        match field {
            F::Id => contains(&self.id, needle),
            F::Name => contains(&self.name, needle),
            F::Description => self
                .description
                .as_deref()
                .is_some_and(|d| contains(d, needle)),
            F::Synonyms => self
                .synonyms
                .iter()
                .flat_map(|s| &s.terms)
                .any(|t| t.as_deref().is_some_and(|t| contains(t, needle))),
        }
    }
}

// ---- loaders ----

pub type DiseaseCache = Cache<String, Option<Disease>>;
static DISEASE_CACHE: LazyLock<DiseaseCache> = LazyLock::new(entity_cache);

#[derive(From)]
pub struct DiseaseLoader {
    ch: ClickHouse,
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
pub async fn load_disease(ctx: &Context<'_>, id: String) -> async_graphql::Result<Option<Disease>> {
    ctx.data_unchecked::<DataLoader<DiseaseLoader>>()
        .load_one(id)
        .await
}

/// A page of diseases, plus a combined view over the studies of all the requested diseases.
#[derive(SimpleObject)]
#[graphql(complex)]
pub struct DiseasePage {
    /// Total number of diseases matching the query.
    pub count: u64,
    /// The diseases on this page.
    pub rows: Vec<Disease>,
    /// Every requested EFO ID, used by `studiesView` regardless of search and paging.
    #[graphql(skip)]
    efo_ids: Vec<String>,
}

#[ComplexObject]
impl DiseasePage {
    /// Flat, sortable and searchable summary of the studies of all the requested diseases, for
    /// tables. Covers every ID in `efoIds`, ignoring the diseases' `search` and `page`. A study
    /// linked to several of the diseases appears once.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn studies_view(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default = false,
            desc = "Whether to add studies for ontology-descendant diseases in the result."
        )]
        enable_indirect: bool,
        #[graphql(
            default_with = "ds::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<ds::SortField>>,
        #[graphql(desc = "Case-insensitive text search across the studies' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<ds::Search>,
        #[graphql(desc = "Filter criteria to apply.")] filter: Option<ds::DiseaseStudiesFilter>,
        #[graphql(default, desc = "Pagination for the studies.")] page: Page,
    ) -> async_graphql::Result<Paged<ds::DiseaseStudy>> {
        let query_params = ds::QueryParams::new(
            self.efo_ids.clone(),
            enable_indirect,
            filter,
            sort,
            search,
            page.index,
            page.size,
        );
        ds::load_disease_studies(ctx, query_params).await
    }
}

// ---- resolvers ----

#[derive(Default)]
pub struct DiseaseQuery;

#[Object]
impl DiseaseQuery {
    /// Fetch diseases by EFO ID.
    #[graphql(complexity = "capped_page_complexity(page, efo_ids.len(), child_complexity)")]
    async fn diseases(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "List of EFO IDs of diseases to fetch.")] efo_ids: Vec<String>,
        #[graphql(desc = "Case-insensitive text search across the diseases' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<Search<DiseaseSearchField>>,
        #[graphql(
            default_with = "DiseaseSortField::default_sort()",
            desc = "List of sort statements. Defaults to ID, ascending."
        )]
        sort: Vec<Sort<DiseaseSortField>>,
        #[graphql(default, desc = "Pagination for the diseases.")] page: Page,
    ) -> async_graphql::Result<DiseasePage> {
        let items = load_diseases(ctx, &efo_ids).await?;
        let Paged { count, rows } = items
            .query()
            .search(search)
            .sort(sort.as_ref())
            .paginate(page);
        Ok(DiseasePage {
            count,
            rows,
            efo_ids,
        })
    }

    async fn disease(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "EFO ID of the disease to fetch.")] efo_id: String,
    ) -> async_graphql::Result<Option<Disease>> {
        ctx.data_unchecked::<DataLoader<DiseaseLoader>>()
            .load_one(efo_id)
            .await
    }
}

#[ComplexObject]
impl Disease {
    /// Clinical indications for this disease as reported by clinical trial records.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn drug_and_clinical_candidates(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "clinical_indication::SortField::default_sort()",
            validator(custom = "crate::query::sort::UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<crate::query::sort::Sort<clinical_indication::SortField>>,
        #[graphql(desc = "Case-insensitive text search on the drug name.")] search: Option<
            clinical_indication::Search,
        >,
        #[graphql(default, desc = "Pagination for the drug and clinical candidates.")] page: Page,
    ) -> async_graphql::Result<Paged<ClinicalIndication>> {
        let key = clinical_indication::QueryParams::new(
            self.id.clone(),
            sort,
            search,
            page.index,
            page.size,
        );
        load_clinical_indications_from_disease(ctx, key).await
    }

    /// Direct parent terms in the disease ontology.
    async fn parents(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Disease>> {
        load_diseases(ctx, &self.parents).await
    }

    /// Direct child terms in the disease ontology.
    async fn children(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Disease>> {
        load_diseases(ctx, &self.children).await
    }

    /// All ancestor diseases up to the top-level therapeutic area.
    async fn resolved_ancestors(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Disease>> {
        load_diseases(ctx, &self.ancestors).await
    }

    /// Clinical signs and symptoms observed in diseases or phenotypes. Signs and symptoms are
    /// integrated from multiple sources including EFO, MONDO and HPO.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn phenotypes(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for the phenotypes.")] page: Page,
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
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
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
            default_with = "DISEASE_INDIRECT_WEIGHT_DEFAULT",
            validator(minimum = 0.0, maximum = 1.0),
            desc = "Weight applied to indirect evidence (from descendant diseases), between 0 and \
                    1. Default is 1."
        )]
        indirect_weight: f64,

        #[graphql(
            default,
            desc = "Optional list of datasource policy overrides with changes to the defaults."
        )]
        datasource_policy_overrides: Vec<DatasourcePolicyOverride>,

        #[graphql(
            default,
            desc = "Ordering for the associations. Can either be `score` to use the overall \
                    association score (default), or datasource id (e.g., `impc`)"
        )]
        sort: AssociationSort,

        #[graphql(default, desc = "Pagination for the associations.")] page: Page,
    ) -> async_graphql::Result<Paged<TargetAssociation>> {
        load_associations::<Disease>(
            ctx,
            &self.id,
            &AssociationArguments {
                bs,
                b_filter,
                facet_filters,
                include_measurements: None, // Used in target to include measurement diseases
                indirect_weight,
                datasource_policy_overrides,
                sort,
                page,
            },
        )
        .await
    }

    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn evidences(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "A list of target ids to filter evidences on.")] ensembl_ids: Vec<String>,
        #[graphql(
            default,
            desc = "A list of datasources to narrow down the selection of evidences."
        )]
        datasource_ids: Vec<Datasource>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> async_graphql::Result<Paged<Evidence>> {
        let all = load_evidences(
            ctx,
            EvidenceKey {
                efo_ids: vec![self.id.clone()],
                ensembl_ids,
                datasource_ids,
            },
        )
        .await?;
        Ok(all.query().paginate(page))
    }

    /// Sorted, searched and paged evidence tables for this disease and the given targets, one
    /// field per datasource.
    async fn evidence_views(
        &self,
        #[graphql(desc = "A list of target ids to filter evidences on.")] ensembl_ids: Vec<String>,
        #[graphql(
            default = true,
            desc = "Whether to include evidence for ontology-descendant diseases."
        )]
        enable_indirect: bool,
    ) -> EvidenceViews {
        EvidenceViews::new(EvidenceScope::new(
            vec![self.id.clone()],
            ensembl_ids,
            enable_indirect,
        ))
    }

    /// Association time series.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn association_time_series(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Ensembl ID of the target.")] ensembl_id: String,
        #[graphql(desc = "Whether to include only direct associations.")] is_direct: bool,
        #[graphql(desc = "Aggregation types.")] aggregation_types: Option<Vec<AggregationType>>,
        #[graphql(desc = "Year at the lower end of the filter.")] start_year: Option<i32>,
        #[graphql(desc = "Year at the higher end of the filter.")] end_year: Option<i32>,
        #[graphql(default, desc = "Pagination for the associations time series.")] page: Page,
    ) -> async_graphql::Result<Paged<AssociationTimeseries>> {
        load_association_timeseries(
            ctx,
            TimeseriesKey {
                disease_id: self.id.clone(),
                target_id: ensembl_id,
            },
            &AssociationTimeseriesArguments {
                is_direct,
                aggregation_types,
                start_year,
                end_year,
                page,
            },
        )
        .await
    }

    /// Return the list of publications that mention the main entity, alone or in combination with
    /// other entities.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn literature_ocurrences(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            desc = "Optional list of IDs (EFO disease IDs, Ensembl gene IDs, or ChEMBL molecule IDs) that the
                    literature must mention along with the main entity. The resulting list of publications will
                    be narrowed down to those papers mentioning the main entity and **ALL** IDs included in this
                    list."
        )]
        additional_ids: Option<Vec<String>>,
        #[graphql(desc = "Year at the lower end of the filter.")] start_year: Option<u32>,
        #[graphql(
            desc = "Month at the lower end of the filter (1–12). Defaults to 1. Ignored if `startYear` is not set.",
            validator(minimum = 1, maximum = 12)
        )]
        start_month: Option<u32>,
        #[graphql(desc = "Year at the higher end of the filter.")] end_year: Option<u32>,
        #[graphql(
            desc = "Month at the higher end of the filter (1–12). Defaults to 12. Ignored if `endYear` is not set.",
            validator(minimum = 1, maximum = 12)
        )]
        end_month: Option<u32>,
        #[graphql(default, desc = "Pagination for the publications.")] page: Page,
    ) -> async_graphql::Result<PagedWithStats<Publication>> {
        let mut ids = additional_ids.unwrap_or_default();
        ids.push(self.id.clone());

        load_publications(
            ctx,
            PublicationArguments {
                ids,
                start_year,
                start_month,
                end_year,
                end_month,
                page,
            },
        )
        .await?
        .ok_or_else(|| "missing publication".into())
    }

    /// Semantically similar diseases based on a PubMed word embedding model.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn similar_entities(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "List of EFO disease IDs.")] additional_ids: Option<Vec<String>>,
        #[graphql(desc = "List of entity names to search for (target, disease, drug,...)")]
        entity_names: Option<Vec<String>>,
        #[graphql(
            desc = "Threshold similarity between 0 and 1",
            validator(minimum = 0.0, maximum = 1.0)
        )]
        threshold: Option<f64>,
        #[graphql(default, desc = "Pagination for similar entities.")] page: Page,
    ) -> async_graphql::Result<Paged<Similarity>> {
        let args = SimilarEntityArguments {
            id: self.id.clone(),
            ids: additional_ids.unwrap_or_default(),
            categories: entity_names.unwrap_or_default(),
            threshold: threshold.unwrap_or(0.1),
            page,
        };
        let entities = load_similar_entities(ctx, args).await?;
        Ok(entities)
    }

    /// Open Targets (OTAR) projects linked to this disease. Data only available in Partner Platform
    /// Preview (PPP).
    #[cfg(feature = "product-ppp")]
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn otar_projects(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for the publications.")] page: Page,
    ) -> async_graphql::Result<Paged<OtarProject>> {
        load_otar_projects(ctx, self.id.clone())
            .await
            .map(|pr| pr.query().paginate(page))
    }
    /// List of major therapeutic areas the disease belongs to.
    async fn therapeutic_areas(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Disease>> {
        load_diseases(ctx, &self.therapeutic_areas).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diseases() -> Vec<Disease> {
        let mut diseases: Vec<_> = [
            ("EFO_1", "Heart syndrome", None),
            ("EFO_2", "Condition B", Some("HEART defect")),
            ("EFO_3", "Condition C", None),
            ("EFO_4", "Café-au-lait spots", None),
            ("EFO_5", "Heart disease", Some("heart issue")),
        ]
        .into_iter()
        .map(|(id, name, description)| Disease {
            id: id.into(),
            name: name.into(),
            description: description.map(str::to_owned),
            is_therapeutic_area: false,
            therapeutic_areas: vec![],
            parents: vec![],
            children: vec![],
            ancestors: vec![],
            descendants: vec![],
            synonyms: vec![],
            obsolete_terms: vec![],
            db_x_refs: vec![],
            direct_location_ids: vec![],
            indirect_location_ids: vec![],
            study_ids: vec![],
            indirect_study_ids: vec![],
        })
        .collect();
        diseases[2].synonyms = vec![
            DiseaseSynonym {
                relation: "exact".into(),
                terms: vec![None, Some("Cardiac finding".into())],
            },
            DiseaseSynonym {
                relation: "related".into(),
                terms: vec![None, Some("hEaRt disorder".into())],
            },
        ];
        diseases[3].synonyms = vec![DiseaseSynonym {
            relation: "exact".into(),
            terms: vec![None],
        }];
        diseases
    }

    fn search_ids(search: Option<Search<DiseaseSearchField>>) -> Vec<String> {
        diseases()
            .query()
            .search(search)
            .into_vec()
            .into_iter()
            .map(|disease| disease.id)
            .collect()
    }

    #[test]
    fn absent_and_blank_searches_keep_rows_with_missing_descriptions() {
        let expected = vec!["EFO_1", "EFO_2", "EFO_3", "EFO_4", "EFO_5"];
        assert_eq!(search_ids(None), expected);
        for text in ["", " \t\n", "\u{2003}"] {
            assert_eq!(
                search_ids(Some(Search {
                    text: text.into(),
                    fields: vec![DiseaseSearchField::Description],
                })),
                expected,
                "text: {text:?}"
            );
        }
    }

    #[test]
    fn search_trims_text_and_matches_case_insensitive_substrings() {
        for (text, expected) in [
            (" \nEaRt\t ", vec!["EFO_1", "EFO_5"]),
            (" \tCAFÉ-AU\n", vec!["EFO_4"]),
            ("pulmonary", vec![]),
        ] {
            assert_eq!(
                search_ids(Some(Search {
                    text: text.into(),
                    fields: vec![DiseaseSearchField::Name],
                })),
                expected,
                "text: {text:?}"
            );
        }
    }

    #[test]
    fn search_matches_any_selected_field_and_returns_each_row_once() {
        use DiseaseSearchField::{Description, Id, Name, Synonyms};

        for (fields, expected) in [
            (vec![Name], vec!["EFO_1", "EFO_5"]),
            (vec![Description], vec!["EFO_2", "EFO_5"]),
            (vec![Name, Description], vec!["EFO_1", "EFO_2", "EFO_5"]),
            (vec![Description, Synonyms], vec!["EFO_2", "EFO_3", "EFO_5"]),
            (vec![Id], vec![]),
        ] {
            let search = Search {
                text: "heart".into(),
                fields,
            };
            assert_eq!(search_ids(Some(search.clone())), expected, "{search:?}");
        }
    }

    #[test]
    fn synonym_search_checks_nested_groups_and_skips_missing_terms() {
        for (text, expected) in [
            ("cardiac", vec!["EFO_3"]),
            ("HEART disorder", vec!["EFO_3"]),
            ("missing", vec![]),
        ] {
            assert_eq!(
                search_ids(Some(Search {
                    text: text.into(),
                    fields: vec![DiseaseSearchField::Synonyms],
                })),
                expected,
                "text: {text:?}"
            );
        }
    }
}
