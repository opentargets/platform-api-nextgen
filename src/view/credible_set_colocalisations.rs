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

/// A colocalisation of this credible set with another one, as shown in the credible set page's
/// GWAS and molQTL colocalisation tables. Study, lead variant and P-value fields describe the
/// **other** credible set.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/credible_set_colocalisations.sql`. Update both together.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct CredibleSetColocalisation {
    /// This credible set (`StudyLocus`), on the left side of the colocalisation pair.
    study_locus_id: String,
    /// The other credible set (`StudyLocus`) in the colocalisation pair.
    other_study_locus_id: String,
    /// Type of the other credible set's study (e.g. gwas, eqtl, pqtl).
    study_type: StudyType,
    /// Study of the other credible set.
    study_id: String,
    /// Source project of the other credible set's study.
    project_id: String,
    /// Trait, as reported by the source, of the other credible set's study.
    trait_from_source: String,
    /// First author of the publication of the other credible set's study.
    publication_first_author: Option<String>,
    /// Condition of the other credible set's molQTL study (e.g. naive).
    condition: Option<String>,
    /// Gene under study in the other credible set's molQTL study.
    target_id: Option<String>,
    /// Approved symbol of `targetId`. Null when the target is unknown.
    target_approved_symbol: Option<String>,
    /// Tissue or cell type of the other credible set's molQTL study.
    biosample_id: Option<String>,
    /// Name of `biosampleId`. Null when the biosample isn't in the biosample index.
    biosample_name: Option<String>,
    /// Description of `biosampleId`.
    biosample_description: Option<String>,
    /// Whether the other credible set is a trans-QTL.
    is_trans_qtl: Option<bool>,
    /// Lead variant of the other credible set.
    lead_variant_id: String,
    /// Chromosome of the lead variant.
    lead_chromosome: Chromosome,
    /// Position of the lead variant (`GRCh38`).
    lead_position: u32,
    /// Mantissa of the other credible set's lead variant P-value.
    p_value_mantissa: f64,
    /// Exponent of the other credible set's lead variant P-value.
    p_value_exponent: i32,
    /// Method used to estimate colocalisation (e.g. coloc, eCAVIAR).
    colocalisation_method: String,
    /// Number of variants intersecting between the two credible sets.
    number_colocalising_variants: u32,
    /// Posterior probability that both traits are associated, but with different causal variants
    /// (H3). Used in coloc method.
    h3: f64,
    /// Posterior probability that both traits are associated and share a causal variant (H4).
    /// Used in coloc method.
    h4: f64,
    /// Colocalisation posterior probability (CLPP) score estimating the probability of shared
    /// causal variants. Used in eCAVIAR method.
    clpp: f64,
    /// Average sign of the beta ratio between colocalised variants (directionality).
    beta_ratio_sign_average: f64,
}

impl Table for CredibleSetColocalisation {
    type Col = CredibleSetColocalisationCol;
    const NAME: &'static str = include_str!("../sql/credible_set_colocalisations.sql");
    // Unique within a credible set, which is all a page ever holds.
    const TIE_BREAKER: &'static str = "otherStudyLocusId";
}

columns!(CredibleSetColocalisationCol {
    StudyLocusId               => "studyLocusId"               : Str,
    // Enum8 in ClickHouse, compared against its names, e.g. 'eqtl'.
    StudyType                  => "studyType"                  : Str,
    StudyId                    => "studyId"                    : Str,
    TraitFromSource            => "traitFromSource"            : Str,
    PublicationFirstAuthor     => "publicationFirstAuthor"     : Str,
    TargetApprovedSymbol       => "targetApprovedSymbol"       : Str,
    BiosampleName              => "biosampleName"              : Str,
    LeadVariantId              => "leadVariantId"              : Str,
    LeadChromosome             => "leadChromosome"             : I64,
    LeadPosition               => "leadPosition"               : I64,
    PValueMantissa             => "pValueMantissa"             : F64,
    PValueExponent             => "pValueExponent"             : I64,
    NumberColocalisingVariants => "numberColocalisingVariants" : I64,
    H3                         => "h3"                         : F64,
    H4                         => "h4"                         : F64,
    Clpp                       => "clpp"                       : F64,
});

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    study_locus_id: String,
    /// Never empty: the GraphQL argument requires at least one.
    study_types: Vec<StudyType>,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(
        study_locus_id: String,
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
            study_locus_id,
            study_types,
            sort,
            search: search.and_then(Search::normalise),
            index,
            size,
        }
    }

    fn to_spec(&self) -> Result<PageSpec<CredibleSetColocalisation>> {
        let study_types = self
            .study_types
            .iter()
            .map(|&t| <&str>::from(t).to_owned())
            .collect();
        // The first filter is pushed down to a key lookup on `colocalisation`.
        let mut filters = vec![
            Filter::new(
                CredibleSetColocalisationCol::StudyLocusId,
                Op::Eq,
                Value::Str(self.study_locus_id.clone()),
            )
            .map_err(Error::new)?,
            Filter::in_str(CredibleSetColocalisationCol::StudyType, study_types)
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
#[graphql(name = "CredibleSetColocalisationsSortField")]
pub enum SortField {
    /// By chromosome, then position, then variant ID.
    LeadVariant,
    /// By exponent, then mantissa.
    PValue,
    NumberColocalisingVariants,
    H3,
    H4,
    Clpp,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::H4,
    direction: SortDirection::Descending,
}];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<CredibleSetColocalisationCol>> {
        use CredibleSetColocalisationCol as C;
        let cols: &[C] = match self {
            Self::LeadVariant => &[C::LeadChromosome, C::LeadPosition, C::LeadVariantId],
            Self::PValue => &[C::PValueExponent, C::PValueMantissa],
            Self::NumberColocalisingVariants => &[C::NumberColocalisingVariants],
            Self::H3 => &[C::H3],
            Self::H4 => &[C::H4],
            Self::Clpp => &[C::Clpp],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against. The GWAS and molQTL tables each pass their
    /// own `fields`.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "CredibleSetColocalisationsSearchField")]
    pub enum SearchField => CredibleSetColocalisationCol {
        StudyId,
        TraitFromSource,
        PublicationFirstAuthor,
        TargetApprovedSymbol,
        BiosampleName,
        LeadVariantId,
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
    colocalisations: Paged<CredibleSetColocalisation>,
}

#[derive(From)]
pub struct CredibleSetColocalisationsLoader {
    ch: ClickHouse,
}

impl CredibleSetColocalisationsLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<QueryParams> for CredibleSetColocalisationsLoader {
    type Value = Paged<CredibleSetColocalisation>;
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
            .map(|row| (keys[row.key].clone(), row.colocalisations))
            .collect())
    }
}

/// # Errors
/// If the `ClickHouse` query fails.
pub async fn load_credible_set_colocalisations(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<CredibleSetColocalisation>> {
    Ok(ctx
        .data_unchecked::<DataLoader<CredibleSetColocalisationsLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
