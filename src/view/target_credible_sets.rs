use std::collections::HashMap;

use async_graphql::{
    Context, Enum, Error, Result, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;
use strum::EnumIter;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{study::StudyType, variant::Chromosome},
    query::{
        paginate::Paged,
        sort::{Sort, SortDirection},
        sql::{
            Column, Filter, Op, OrderBy, PageSpec, Table, Value, ValueKind, build, column_subset,
            columns,
        },
    },
};

// ---- model ----

/// A molQTL credible set, as shown in the target page's credible sets table.
///
/// Rows are decoded by position, so field order must match the column order of the
/// `target_credible_sets` table. Update this struct if that table's columns change.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TargetCredibleSet {
    target_id: String,
    study_locus_id: String,
    p_value_mantissa: f64,
    p_value_exponent: i32,
    beta: Option<f64>,
    finemapping_method: String,
    confidence: String,
    is_trans_qtl: Option<bool>,
    lead_variant_id: String,
    lead_chromosome: Chromosome,
    lead_position: u32,
    lead_reference_allele: String,
    lead_alternate_allele: String,
    study_id: String,
    study_type: StudyType,
    condition: Option<String>,
    biosample_id: Option<String>,
    /// Empty when the biosample is not in the biosample index.
    biosample_name: String,
    /// Number of variants in the credible set.
    locus_size: u64,
}

impl Table for TargetCredibleSet {
    type Col = TargetCredibleSetCol;
    const NAME: &'static str = "target_credible_sets";
    const TIE_BREAKER: &'static str = "studyLocusId";
}

columns!(TargetCredibleSetCol {
    TargetId            => "targetId"            : Str,
    PValueMantissa      => "pValueMantissa"      : F64,
    PValueExponent      => "pValueExponent"      : I64,
    Beta                => "beta"                : F64,
    FinemappingMethod   => "finemappingMethod"   : Str,
    Confidence          => "confidence"          : Str,
    LeadVariantId       => "leadVariantId"       : Str,
    LeadChromosome      => "leadChromosome"      : I64,
    LeadPosition        => "leadPosition"        : I64,
    LeadReferenceAllele => "leadReferenceAllele" : Str,
    LeadAlternateAllele => "leadAlternateAllele" : Str,
    StudyId             => "studyId"             : Str,
    // Enum8 in ClickHouse, compared against its names, e.g. 'eqtl'.
    StudyType           => "studyType"           : Str,
    // Sorting on the Enum8 itself would follow its numeric order, not the name.
    StudyTypeName       => "toString(studyType)" : Str,
    Condition           => "condition"           : Str,
    BiosampleName       => "biosampleName"       : Str,
    LocusSize           => "locusSize"           : I64,
});

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    target_id: String,
    /// Empty means every study type.
    study_types: Vec<StudyType>,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(
        target_id: String,
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
            target_id,
            study_types,
            sort,
            search: search.and_then(Search::normalise),
            index,
            size,
        }
    }

    fn to_spec(&self) -> Result<PageSpec<TargetCredibleSet>> {
        // Filters on the table's sort-key prefix (targetId).
        let mut filters = vec![
            Filter::new(
                TargetCredibleSetCol::TargetId,
                Op::Eq,
                Value::Str(self.target_id.clone()),
            )
            .map_err(Error::new)?,
        ];
        if !self.study_types.is_empty() {
            let study_types = self
                .study_types
                .iter()
                .map(|&t| <&str>::from(t).to_owned())
                .collect();
            filters.push(
                Filter::in_str(TargetCredibleSetCol::StudyType, study_types).map_err(Error::new)?,
            );
        }
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
#[graphql(name = "TargetCredibleSetsSortField")]
pub enum SortField {
    /// By chromosome, then position, then alleles.
    LeadVariant,
    /// Alphabetically by study ID.
    Study,
    /// Alphabetically by study type name.
    StudyType,
    /// Alphabetically by biosample (affected tissue/cell) name.
    BiosampleName,
    /// Alphabetically.
    Condition,
    /// By exponent, then mantissa.
    PValue,
    Beta,
    /// Alphabetically.
    Confidence,
    /// Alphabetically.
    FinemappingMethod,
    /// Number of variants in the credible set.
    LocusSize,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::PValue,
    direction: SortDirection::Ascending,
}];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<TargetCredibleSetCol>> {
        use TargetCredibleSetCol as C;
        let cols: &[C] = match self {
            Self::LeadVariant => &[
                C::LeadChromosome,
                C::LeadPosition,
                C::LeadReferenceAllele,
                C::LeadAlternateAllele,
            ],
            Self::Study => &[C::StudyId],
            Self::StudyType => &[C::StudyTypeName],
            Self::BiosampleName => &[C::BiosampleName],
            Self::Condition => &[C::Condition],
            Self::PValue => &[C::PValueExponent, C::PValueMantissa],
            Self::Beta => &[C::Beta],
            Self::Confidence => &[C::Confidence],
            Self::FinemappingMethod => &[C::FinemappingMethod],
            Self::LocusSize => &[C::LocusSize],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "TargetCredibleSetsSearchField")]
    pub enum SearchField => TargetCredibleSetCol {
        LeadVariantId,
        StudyId,
        StudyType,
        BiosampleName,
        Condition,
        FinemappingMethod,
    }
}

/// Case-insensitive text search over the `SearchField`s.
pub type Search = crate::query::search::Search<SearchField>;

// ---- loader ----

/// One row of the batched query: the page of results for `keys[key]`.
#[derive(Debug, Clone, Row, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRow {
    key: usize,
    credible_sets: Paged<TargetCredibleSet>,
}

#[derive(From)]
pub struct TargetCredibleSetsLoader {
    ch: ClickHouse,
}

impl TargetCredibleSetsLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<QueryParams> for TargetCredibleSetsLoader {
    type Value = Paged<TargetCredibleSet>;
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
pub async fn load_target_credible_sets(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<TargetCredibleSet>> {
    Ok(ctx
        .data_unchecked::<DataLoader<TargetCredibleSetsLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
