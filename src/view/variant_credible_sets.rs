use std::collections::HashMap;

use async_graphql::{
    Context, Enum, Error, InputObject, Object, Result, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;
use strum::{EnumIter, IntoEnumIterator};

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{study::StudyType, variant::Chromosome},
    query::{
        paginate::{Page, Paged, page_complexity},
        sort::{Sort, SortDirection, UniqueSortKeys},
        sql::{
            Column, Filter, Op, OrderBy, PageSpec, Table, Value, ValueKind, build, column_subset,
            columns,
        },
    },
};

// ---- model ----

/// A disease (or therapeutic area) linked to a credible set.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[graphql(name = "VariantCredibleSetDisease")]
pub struct DiseaseRef {
    id: String,
    name: String,
}

/// Rows are decoded by position, so field order must match the column order of the
/// `variant_credible_sets` table. Update this struct if that table's columns change.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct VariantCredibleSet {
    study_type: StudyType,
    variant_id: String,
    study_locus_id: String,
    posterior_probability: f64,
    locus_size: u64,
    p_value_mantissa: f64,
    p_value_exponent: i32,
    beta: Option<f64>,
    finemapping_method: String,
    confidence: String,
    lead_variant_id: String,
    lead_chromosome: Chromosome,
    lead_position: u32,
    lead_reference_allele: String,
    lead_alternate_allele: String,
    study_id: String,
    trait_from_source: String,
    diseases: Vec<DiseaseRef>,
    disease_therapeutic_areas: Vec<DiseaseRef>,
    top_l2_g_gene_id: Option<String>,
    top_l2_g_approved_symbol: Option<String>,
    top_l2_g_score: Option<f64>,
}

impl Table for VariantCredibleSet {
    type Col = VariantCredibleSetCol;
    const NAME: &'static str = "variant_credible_sets";
    const TIE_BREAKER: &'static str = "studyLocusId";
}

columns!(VariantCredibleSetCol {
    VariantId                   => "variantId"                   : Str,
    StudyLocusId                => "studyLocusId"                : Str,
    PValueMantissa              => "pValueMantissa"              : F64,
    PValueExponent              => "pValueExponent"              : I64,
    FinemappingMethod           => "finemappingMethod"           : Str,
    Confidence                  => "confidence"                  : Str,
    LeadVariantId               => "leadVariantId"               : Str,
    LeadChromosome              => "leadChromosome"              : I64,
    LeadPosition                => "leadPosition"                : I64,
    LeadReferenceAllele         => "leadReferenceAllele"         : Str,
    LeadAlternateAllele         => "leadAlternateAllele"         : Str,
    StudyId                     => "studyId"                     : Str,
    // Enum8 in ClickHouse, compared against its names, e.g. 'gwas'.
    StudyType                   => "studyType"                   : Str,
    TraitFromSource             => "traitFromSource"             : Str,
    // Pull one field out of each (id, name) tuple, giving a plain string array to search.
    DiseaseIds                  => "arrayMap(d -> d.id, diseases)"                  : StrArray,
    DiseaseNames                => "arrayMap(d -> d.name, diseases)"                : StrArray,
    DiseaseTherapeuticAreaIds   => "arrayMap(d -> d.id, diseaseTherapeuticAreas)"   : StrArray,
    DiseaseTherapeuticAreaNames => "arrayMap(d -> d.name, diseaseTherapeuticAreas)" : StrArray,
    TopL2gGeneId                => "topL2GGeneId"                : Str,
    TopL2gApprovedSymbol        => "topL2GApprovedSymbol"        : Str,
    TopL2gScore                 => "topL2GScore"                 : F64,
});

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    variant_id: String,
    study_types: Vec<StudyType>,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(
        variant_id: String,
        mut study_types: Vec<StudyType>,
        sort: Vec<Sort<SortField>>,
        search: Option<Search>,
        index: u32,
        size: u32,
    ) -> Self {
        // Sorted and deduped so equivalent requests share a `DataLoader` key.
        study_types.sort_unstable();
        study_types.dedup();
        Self {
            variant_id,
            study_types,
            sort,
            search: search.and_then(Search::normalise),
            index,
            size,
        }
    }

    fn to_spec(&self) -> Result<PageSpec<VariantCredibleSet>> {
        let study_types = self
            .study_types
            .iter()
            .map(|&t| <&str>::from(t).to_owned())
            .collect();
        // Filters on the table's sort-key prefix (studyType, variantId).
        let mut filters = vec![
            Filter::in_str(VariantCredibleSetCol::StudyType, study_types).map_err(Error::new)?,
            Filter::new(
                VariantCredibleSetCol::VariantId,
                Op::Eq,
                Value::Str(self.variant_id.clone()),
            )
            .map_err(Error::new)?,
        ];
        filters.extend(self.search.as_ref().map(Search::to_filter).transpose()?);
        Ok(PageSpec {
            filters,
            order_by: self
                .sort
                .iter()
                .flat_map(|s| s.key.as_order_by(s.direction))
                .collect(),
            offset: u64::from(self.index) * u64::from(self.size),
            limit: u64::from(self.size),
        })
    }
}

// ---- sort ----

/// Fields available for sorting.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum, Hash)]
#[graphql(name = "VariantCredibleSetsSortField")]
pub enum SortField {
    // Default naming would give `TOP_L_2G_SCORE`.
    #[graphql(name = "TOP_L2G_SCORE")]
    TopL2gScore,
    PValue,
    LeadVariant,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::TopL2gScore,
    direction: SortDirection::Descending,
}];

impl SortField {
    fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<VariantCredibleSetCol>> {
        use VariantCredibleSetCol as C;
        match self {
            Self::TopL2gScore => OrderBy::each(&[C::TopL2gScore], dir),
            Self::PValue => OrderBy::each(&[C::PValueExponent, C::PValueMantissa], dir),
            Self::LeadVariant => OrderBy::each(
                &[
                    C::LeadChromosome,
                    C::LeadPosition,
                    C::LeadReferenceAllele,
                    C::LeadAlternateAllele,
                ],
                dir,
            ),
        }
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against.
    ///
    /// Leaves out `variantId` (the same on every row) and the alleles (single letters
    /// that would match almost every search).
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "VariantCredibleSetsSearchField")]
    pub enum SearchField => VariantCredibleSetCol {
        StudyLocusId,
        StudyId,
        LeadVariantId,
        FinemappingMethod,
        Confidence,
        TraitFromSource,
        // Default naming would give `TOP_L_2G_...`.
        #[graphql(name = "TOP_L2G_GENE_ID")]
        TopL2gGeneId,
        #[graphql(name = "TOP_L2G_APPROVED_SYMBOL")]
        TopL2gApprovedSymbol,
        DiseaseIds,
        DiseaseNames,
        DiseaseTherapeuticAreaIds,
        DiseaseTherapeuticAreaNames,
    }
}

impl SearchField {
    /// Every searchable field; the default when the client doesn't pick any.
    fn all() -> Vec<Self> { Self::iter().collect() }
}

/// Case-insensitive text search over the credible sets.
#[derive(Debug, Clone, Eq, PartialEq, Hash, InputObject)]
#[graphql(name = "VariantCredibleSetsSearch")]
pub struct Search {
    /// Text to look for. Matches if any of `fields` contains it.
    #[graphql(validator(chars_max_length = 200))]
    text: String,
    /// Fields to search in. Defaults to all of them.
    #[graphql(default_with = "SearchField::all()", validator(min_items = 1))]
    fields: Vec<SearchField>,
}

impl Search {
    /// Trims the text and sorts/dedupes the fields, so equivalent searches share
    /// a `DataLoader` key. A blank search becomes `None`, the same as no search.
    fn normalise(mut self) -> Option<Self> {
        self.text = self.text.trim().to_owned();
        if self.text.is_empty() {
            return None;
        }
        self.fields.sort_unstable();
        self.fields.dedup();
        Some(self)
    }

    fn to_filter(&self) -> Result<Filter<VariantCredibleSetCol>> {
        let cols = self.fields.iter().map(|f| f.as_col()).collect();
        Filter::search(cols, self.text.clone()).map_err(Error::new)
    }
}

// ---- loader ----

/// One row of the batched query: the page of results for `keys[key]`.
#[derive(Debug, Clone, Row, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRow {
    key: usize,
    credible_sets: Paged<VariantCredibleSet>,
}

#[derive(From)]
pub struct VariantCredibleSetsLoader {
    ch: ClickHouse,
}

impl VariantCredibleSetsLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<QueryParams> for VariantCredibleSetsLoader {
    type Value = Paged<VariantCredibleSet>;
    type Error = Error;

    async fn load(&self, keys: &[QueryParams]) -> Result<HashMap<QueryParams, Self::Value>> {
        let rendered = keys
            .iter()
            .map(|k| k.to_spec().map(|spec| spec.render()))
            .collect::<Result<Vec<_>>>()?;

        let rows = build(self.ch.client(), &rendered)
            .fetch_all::<BatchRow>()
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| (keys[row.key].clone(), row.credible_sets))
            .collect())
    }
}

/// # Errors
/// If the `ClickHouse` query fails.
pub async fn load_variant_credible_sets(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<VariantCredibleSet>> {
    Ok(ctx
        .data_unchecked::<DataLoader<VariantCredibleSetsLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}

// ---- resolver ----

#[derive(Default)]
pub struct VariantCredibleSetsQuery;

#[Object]
impl VariantCredibleSetsQuery {
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn variant_credible_sets(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "variant ID to fetch credible sets for.")] variant_id: String,
        #[graphql(
            validator(min_items = 1),
            desc = "Study types to include, e.g. `[gwas]` or the QTL types."
        )]
        study_types: Vec<StudyType>,
        #[graphql(
            default_with = "SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<SortField>>,
        #[graphql(desc = "Case-insensitive text search across the credible sets' text \
                          fields. Searches all fields unless `fields` is given.")]
        search: Option<Search>,
        #[graphql(default, desc = "Pagination for the credible sets.")] page: Page,
    ) -> Result<Paged<VariantCredibleSet>> {
        let query_params =
            QueryParams::new(variant_id, study_types, sort, search, page.index, page.size);
        load_variant_credible_sets(ctx, query_params).await
    }
}
