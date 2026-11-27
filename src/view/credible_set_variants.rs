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
    entity::variant::Chromosome,
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

/// A variant in a credible set, as shown in the credible set page's variants table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/credible_set_variants.sql`. Update both together.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct CredibleSetVariant {
    /// Identifier of the credible set.
    credible_set_id: String,
    /// Variant identifier (`chromosome_position_ref_alt`).
    variant_id: String,
    /// Whether this is the credible set's lead variant.
    is_lead: bool,
    /// Chromosome of the variant.
    chromosome: Chromosome,
    /// Position of the variant on the chromosome (`GRCh38`).
    position: u32,
    /// Reference allele of the variant.
    reference_allele: String,
    /// Alternate allele of the variant.
    alternate_allele: String,
    /// Posterior inclusion probability for the variant within this credible set.
    posterior_probability: f32,
    /// Natural log Bayes factor for the variant from fine-mapping.
    log_bayes_factor: Option<f32>,
    /// Mantissa of the P-value for this variant in the credible set.
    p_value_mantissa: Option<f32>,
    /// Exponent of the P-value for this variant in the credible set.
    p_value_exponent: Option<i32>,
    /// Beta coefficient of this variant, as reported by the source. Not comparable across studies.
    reported_beta: Option<f64>,
    /// Standard error of the reported beta.
    reported_standard_error: Option<f64>,
    /// R-squared (LD) between this credible set variant and the lead variant.
    r2_overall: Option<f32>,
    /// Sample size for this variant.
    sample_size: Option<u32>,
    /// Effect allele frequency of this variant.
    effect_allele_frequency: Option<f32>,
    /// Effect of this variant from fine-mapping, conditional on the other signals in the locus.
    conditional_beta: Option<f32>,
    /// Standard error of the conditional beta.
    conditional_standard_error: Option<f32>,
    /// Absolute effect rescaled to be comparable across studies.
    rescaled_abs_beta: Option<f32>,
    /// Standard error of the rescaled effect.
    rescaled_standard_error: Option<f32>,
    /// Direction of the rescaled effect (1 or -1).
    rescaled_direction: Option<i8>,
    /// Sequence Ontology id of the variant's most severe consequence (e.g. `SO:0001583`).
    most_severe_consequence_id: Option<String>,
    /// Sequence Ontology label of the variant's most severe consequence (e.g. missense variant).
    most_severe_consequence_label: Option<String>,
}

impl Table for CredibleSetVariant {
    type Col = CredibleSetVariantCol;
    const NAME: &'static str = include_str!("../sql/credible_set_variants.sql");
    // Unique within a credible set, which is all a page ever holds.
    const TIE_BREAKER: &'static str = "variantId";
}

columns!(CredibleSetVariantCol {
    CredibleSetId              => "credibleSetId"              : Str,
    VariantId                  => "variantId"                  : Str,
    IsLead                     => "isLead"                     : U64,
    Chromosome                => "chromosome"                 : I64,
    Position                   => "position"                   : I64,
    ReferenceAllele            => "referenceAllele"            : Str,
    AlternateAllele            => "alternateAllele"            : Str,
    PosteriorProbability       => "posteriorProbability"       : F64,
    LogBayesFactor             => "logBayesFactor"             : F64,
    PValueMantissa             => "pValueMantissa"             : F64,
    PValueExponent             => "pValueExponent"             : I64,
    ReportedBeta               => "reportedBeta"               : F64,
    ReportedStandardError      => "reportedStandardError"      : F64,
    R2Overall                  => "r2Overall"                  : F64,
    SampleSize                 => "sampleSize"                 : U64,
    EffectAlleleFrequency      => "effectAlleleFrequency"      : F64,
    ConditionalBeta            => "conditionalBeta"            : F64,
    ConditionalStandardError   => "conditionalStandardError"   : F64,
    RescaledAbsBeta            => "rescaledAbsBeta"            : F64,
    RescaledStandardError      => "rescaledStandardError"      : F64,
    RescaledDirection          => "rescaledDirection"          : I64,
    // Search sees spaces, as the table shows them. The returned label keeps underscores.
    MostSevereConsequenceLabel => "replaceAll(mostSevereConsequenceLabel, '_', ' ')" : Str,
});

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    credible_set_id: String,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(
        credible_set_id: String,
        sort: Vec<Sort<SortField>>,
        search: Option<Search>,
        index: u32,
        size: u32,
    ) -> Self {
        Self {
            credible_set_id,
            sort,
            search: search.and_then(Search::normalise),
            index,
            size,
        }
    }

    fn to_spec(&self) -> Result<PageSpec<CredibleSetVariant>> {
        // Pushed down to a key lookup on `credible_sets_locus`.
        let mut filters = vec![
            Filter::new(
                CredibleSetVariantCol::CredibleSetId,
                Op::Eq,
                Value::Str(self.credible_set_id.clone()),
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
#[graphql(name = "CredibleSetVariantsSortField")]
pub enum SortField {
    /// Whether the variant is the credible set's lead variant. Sort descending to put it first.
    IsLead,
    /// By chromosome, then position, then alleles.
    Variant,
    /// By exponent, then mantissa.
    PValue,
    ReportedBeta,
    ReportedStandardError,
    /// R-squared (LD) with the lead variant.
    #[graphql(name = "R2_OVERALL")]
    R2Overall,
    PosteriorProbability,
    LogBayesFactor,
    SampleSize,
    EffectAlleleFrequency,
    /// Effect from fine-mapping, conditional on the other signals in the locus.
    ConditionalBeta,
    ConditionalStandardError,
    /// Absolute effect rescaled to be comparable across studies.
    RescaledAbsBeta,
    RescaledStandardError,
    /// Direction of the rescaled effect (-1 before 1 ascending).
    RescaledDirection,
}

/// By posterior probability, with the lead variant first among ties.
const DEFAULT_SORT: [Sort<SortField>; 2] = [
    Sort {
        key: SortField::PosteriorProbability,
        direction: SortDirection::Descending,
    },
    Sort {
        key: SortField::IsLead,
        direction: SortDirection::Descending,
    },
];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<CredibleSetVariantCol>> {
        use CredibleSetVariantCol as C;
        let cols: &[C] = match self {
            Self::IsLead => &[C::IsLead],
            Self::Variant => &[
                C::Chromosome,
                C::Position,
                C::ReferenceAllele,
                C::AlternateAllele,
            ],
            Self::PValue => &[C::PValueExponent, C::PValueMantissa],
            Self::ReportedBeta => &[C::ReportedBeta],
            Self::ReportedStandardError => &[C::ReportedStandardError],
            Self::R2Overall => &[C::R2Overall],
            Self::PosteriorProbability => &[C::PosteriorProbability],
            Self::LogBayesFactor => &[C::LogBayesFactor],
            Self::SampleSize => &[C::SampleSize],
            Self::EffectAlleleFrequency => &[C::EffectAlleleFrequency],
            Self::ConditionalBeta => &[C::ConditionalBeta],
            Self::ConditionalStandardError => &[C::ConditionalStandardError],
            Self::RescaledAbsBeta => &[C::RescaledAbsBeta],
            Self::RescaledStandardError => &[C::RescaledStandardError],
            Self::RescaledDirection => &[C::RescaledDirection],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "CredibleSetVariantsSearchField")]
    pub enum SearchField => CredibleSetVariantCol {
        VariantId,
        MostSevereConsequenceLabel,
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
    variants: Paged<CredibleSetVariant>,
}

#[derive(From)]
pub struct CredibleSetVariantsLoader {
    ch: ClickHouse,
}

impl CredibleSetVariantsLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<QueryParams> for CredibleSetVariantsLoader {
    type Value = Paged<CredibleSetVariant>;
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
            .map(|row| (keys[row.key].clone(), row.variants))
            .collect())
    }
}

/// # Errors
/// If the `ClickHouse` query fails.
pub async fn load_credible_set_variants(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<CredibleSetVariant>> {
    Ok(ctx
        .data_unchecked::<DataLoader<CredibleSetVariantsLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
