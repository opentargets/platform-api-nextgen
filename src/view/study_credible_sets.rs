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
        paginate::{Page, Paged, page_complexity},
        sort::{Sort, SortDirection, UniqueSortKeys},
        sql::{
            Column, Filter, Op, OrderBy, PageSpec, Table, Value, ValueKind, build, column_subset,
            columns,
        },
    },
};

// ---- model ----

/// A GWAS credible set, as shown in the study page's credible sets table.
///
/// Rows are decoded by position, so field order must match the column order of the
/// `study_credible_sets` table. Update this struct if that table's columns change.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct StudyCredibleSet {
    study_id: String,
    study_locus_id: String,
    lead_variant_id: String,
    lead_chromosome: Chromosome,
    lead_position: u32,
    lead_reference_allele: String,
    lead_alternate_allele: String,
    p_value_mantissa: f64,
    p_value_exponent: i32,
    beta: Option<f64>,
    finemapping_method: String,
    confidence: String,
    /// Number of variants in the credible set.
    locus_size: u64,
    /// Null when the credible set has no L2G predictions.
    top_l2_g_gene_id: Option<String>,
    top_l2_g_approved_symbol: Option<String>,
    top_l2_g_score: Option<f64>,
}

impl Table for StudyCredibleSet {
    type Col = StudyCredibleSetCol;
    const NAME: &'static str = "study_credible_sets";
    const TIE_BREAKER: &'static str = "studyLocusId";
}

columns!(StudyCredibleSetCol {
    StudyId              => "studyId"              : Str,
    LeadVariantId        => "leadVariantId"        : Str,
    LeadChromosome       => "leadChromosome"       : I64,
    LeadPosition         => "leadPosition"         : I64,
    LeadReferenceAllele  => "leadReferenceAllele"  : Str,
    LeadAlternateAllele  => "leadAlternateAllele"  : Str,
    PValueMantissa       => "pValueMantissa"       : F64,
    PValueExponent       => "pValueExponent"       : I64,
    Beta                 => "beta"                 : F64,
    FinemappingMethod    => "finemappingMethod"    : Str,
    Confidence           => "confidence"           : Str,
    LocusSize            => "locusSize"            : I64,
    TopL2gApprovedSymbol => "topL2GApprovedSymbol" : Str,
    TopL2gScore          => "topL2GScore"          : F64,
});

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    study_id: String,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(
        study_id: String,
        sort: Vec<Sort<SortField>>,
        search: Option<Search>,
        index: u32,
        size: u32,
    ) -> Self {
        Self {
            study_id,
            sort,
            search: search.and_then(Search::normalise),
            index,
            size,
        }
    }

    fn to_spec(&self) -> Result<PageSpec<StudyCredibleSet>> {
        // Filters on the table's sort-key prefix (studyId).
        let mut filters = vec![
            Filter::new(
                StudyCredibleSetCol::StudyId,
                Op::Eq,
                Value::Str(self.study_id.clone()),
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
#[graphql(name = "StudyCredibleSetsSortField")]
pub enum SortField {
    /// By chromosome, then position, then alleles.
    LeadVariant,
    /// By exponent, then mantissa.
    PValue,
    Beta,
    /// Alphabetically.
    Confidence,
    /// By the top L2G gene's symbol.
    // Default naming would give `TOP_L_2G_...`.
    #[graphql(name = "TOP_L2G_APPROVED_SYMBOL")]
    TopL2gApprovedSymbol,
    #[graphql(name = "TOP_L2G_SCORE")]
    TopL2gScore,
    /// Number of variants in the credible set.
    LocusSize,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::PValue,
    direction: SortDirection::Ascending,
}];

impl SortField {
    fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<StudyCredibleSetCol>> {
        use StudyCredibleSetCol as C;
        let cols: &[C] = match self {
            Self::LeadVariant => &[
                C::LeadChromosome,
                C::LeadPosition,
                C::LeadReferenceAllele,
                C::LeadAlternateAllele,
            ],
            Self::PValue => &[C::PValueExponent, C::PValueMantissa],
            Self::Beta => &[C::Beta],
            Self::Confidence => &[C::Confidence],
            Self::TopL2gApprovedSymbol => &[C::TopL2gApprovedSymbol],
            Self::TopL2gScore => &[C::TopL2gScore],
            Self::LocusSize => &[C::LocusSize],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "StudyCredibleSetsSearchField")]
    pub enum SearchField => StudyCredibleSetCol {
        LeadVariantId,
        FinemappingMethod,
        // Default naming would give `TOP_L_2G_...`.
        #[graphql(name = "TOP_L2G_APPROVED_SYMBOL")]
        TopL2gApprovedSymbol,
    }
}

impl SearchField {
    /// Every searchable field; the default when the client doesn't pick any.
    fn all() -> Vec<Self> { Self::iter().collect() }
}

/// Case-insensitive text search over the credible sets.
#[derive(Debug, Clone, Eq, PartialEq, Hash, InputObject)]
#[graphql(name = "StudyCredibleSetsSearch")]
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

    fn to_filter(&self) -> Result<Filter<StudyCredibleSetCol>> {
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
    credible_sets: Paged<StudyCredibleSet>,
}

#[derive(From)]
pub struct StudyCredibleSetsLoader {
    ch: ClickHouse,
}

impl StudyCredibleSetsLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<QueryParams> for StudyCredibleSetsLoader {
    type Value = Paged<StudyCredibleSet>;
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
pub async fn load_study_credible_sets(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<StudyCredibleSet>> {
    Ok(ctx
        .data_unchecked::<DataLoader<StudyCredibleSetsLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}

// ---- resolver ----

#[derive(Default)]
pub struct StudyCredibleSetsQuery;

#[Object]
impl StudyCredibleSetsQuery {
    /// GWAS credible sets for a study.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn study_credible_sets(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "GWAS study ID to fetch credible sets for.")] study_id: String,
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
    ) -> Result<Paged<StudyCredibleSet>> {
        let query_params = QueryParams::new(study_id, sort, search, page.index, page.size);
        load_study_credible_sets(ctx, query_params).await
    }
}
