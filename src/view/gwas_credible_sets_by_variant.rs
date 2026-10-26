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
    entity::variant::Chromosome,
    query::{
        paginate::{Page, Paged},
        sort::{Sort, SortDirection, UniqueSortKeys},
        sql::{
            Column, Filter, Op, OrderBy, PageSpec, Table, Value, ValueKind, build, column_subset,
            columns,
        },
    },
};

// ---- model ----

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct GwasCredibleSetsByVariant {
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
    disease_ids: Vec<String>,
    disease_names: Vec<String>,
    disease_therapeutic_area_ids: Vec<Vec<String>>,
    disease_therapeutic_area_names: Vec<Vec<String>>,
    top_l2_g_gene_id: String,
    top_l2_g_approved_symbol: String,
    top_l2_g_score: f64,
}

impl Table for GwasCredibleSetsByVariant {
    type Col = CredibleSetCol;
    const NAME: &'static str = "gwas_credible_sets_by_variant";
    const TIE_BREAKER: &'static str = "studyLocusId";
}

columns!(CredibleSetCol {
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
    TraitFromSource             => "traitFromSource"             : Str,
    DiseaseIds                  => "diseaseIds"                  : StrArray,
    DiseaseNames                => "diseaseNames"                : StrArray,
    DiseaseTherapeuticAreaIds   => "diseaseTherapeuticAreaIds"   : StrArrayArray,
    DiseaseTherapeuticAreaNames => "diseaseTherapeuticAreaNames" : StrArrayArray,
    TopL2gGeneId                => "topL2GGeneId"                : Str,
    TopL2gApprovedSymbol        => "topL2GApprovedSymbol"        : Str,
    TopL2gScore                 => "topL2GScore"                 : F64,
});

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    variant_id: String,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(
        variant_id: String,
        sort: Vec<Sort<SortField>>,
        search: Option<Search>,
        index: u32,
        size: u32,
    ) -> Self {
        Self {
            variant_id,
            sort,
            search: search.and_then(Search::normalise),
            index,
            size,
        }
    }

    fn to_spec(&self) -> Result<PageSpec<GwasCredibleSetsByVariant>> {
        let mut filters = vec![
            Filter::new(
                CredibleSetCol::VariantId,
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
#[graphql(name = "GwasCredibleSetsByVariantSortField")]
pub enum SortField {
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

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<CredibleSetCol>> {
        use CredibleSetCol as C;
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
    #[graphql(name = "GwasCredibleSetsByVariantSearchField")]
    pub enum SearchField => CredibleSetCol {
        StudyLocusId,
        StudyId,
        LeadVariantId,
        FinemappingMethod,
        Confidence,
        TraitFromSource,
        TopL2gGeneId,
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
#[graphql(name = "GwasCredibleSetsByVariantSearch")]
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

    fn to_filter(&self) -> Result<Filter<CredibleSetCol>> {
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
    gwas_credible_sets: Paged<GwasCredibleSetsByVariant>,
}

#[derive(From)]
pub struct GwasCredibleSetsByVariantLoader {
    ch: ClickHouse,
}

impl GwasCredibleSetsByVariantLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<QueryParams> for GwasCredibleSetsByVariantLoader {
    type Value = Paged<GwasCredibleSetsByVariant>;
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
            .map(|row| (keys[row.key].clone(), row.gwas_credible_sets))
            .collect())
    }
}

/// # Errors
/// If the `ClickHouse` query fails.
pub async fn load_gwas_credible_sets_by_variant(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<GwasCredibleSetsByVariant>> {
    Ok(ctx
        .data_unchecked::<DataLoader<GwasCredibleSetsByVariantLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}

// ---- resolver ----

#[derive(Default)]
pub struct GwasCredibleSetsByVariantQuery;

#[Object]
impl GwasCredibleSetsByVariantQuery {
    async fn gwas_credible_sets_by_variant(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "variant ID to fetch credible sets for.")] variant_id: String,
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
    ) -> Result<Paged<GwasCredibleSetsByVariant>> {
        let query_params = QueryParams::new(variant_id, sort, search, page.index, page.size);
        load_gwas_credible_sets_by_variant(ctx, query_params).await
    }
}
