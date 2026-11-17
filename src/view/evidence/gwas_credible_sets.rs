//! GWAS credible sets evidence: fine-mapped GWAS signals whose locus-to-gene (L2G) model
//! prioritises the target, for the disease the study's trait is mapped to.

use std::collections::HashMap;

use async_graphql::{
    Context, Enum, Error, Result, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;
use strum::EnumIter;

use super::EvidenceScope;
use crate::{
    datasource::clickhouse::ClickHouse,
    entity::variant::Chromosome,
    query::{
        paginate::Paged,
        sort::{Sort, SortDirection},
        sql::{Column, OrderBy, PageSpec, Table, ValueKind, build, column_subset, columns},
    },
};

const DATASOURCE: &str = "gwas_credible_sets";

// ---- model ----

/// A GWAS credible sets evidence, as shown in the evidence page's GWAS associations table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/gwas_credible_sets.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceGwasCredibleSets {
    /// Identifier of the disease/target evidence.
    id: String,
    /// Locus-to-gene (L2G) score of the target for the credible set.
    score: f64,
    /// Identifier of the disease the study's trait is mapped to.
    disease_id: String,
    /// Name of the disease. Empty if the disease isn't found.
    disease_name: String,
    /// Identifier of the target prioritised by L2G.
    target_id: String,
    /// Identifier of the credible set (`StudyLocus`) supporting this evidence.
    study_locus_id: Option<String>,
    /// Identifier of the credible set's lead variant.
    lead_variant_id: String,
    /// Chromosome of the lead variant.
    lead_chromosome: Chromosome,
    /// Position of the lead variant (`GRCh38`).
    lead_position: u32,
    /// Reference allele of the lead variant.
    lead_reference_allele: String,
    /// Alternate allele of the lead variant.
    lead_alternate_allele: String,
    /// Mantissa of the lead variant's p-value.
    p_value_mantissa: f64,
    /// Exponent of the lead variant's p-value.
    p_value_exponent: i32,
    /// Effect size of the lead variant.
    beta: Option<f64>,
    /// Fine-mapping method used to derive the credible set.
    finemapping_method: String,
    /// Fine-mapping confidence of the credible set.
    confidence: String,
    #[allow(dead_code, reason = "read only to keep the column order")]
    #[graphql(skip)]
    confidence_rank: u64,
    /// Identifier of the GWAS study.
    study_id: String,
    /// Trait, derived from source, analysed in the study.
    trait_from_source: String,
    /// Identifier of the project the study comes from.
    project_id: String,
    /// First author of the publication that references the study.
    publication_first_author: Option<String>,
    /// Date of the publication that references the study.
    publication_date: Option<String>,
    /// PubMed identifier of the publication that references the study [bioregistry:pubmed].
    pubmed_id: Option<String>,
    /// Number of samples tested in the study.
    n_samples: Option<u32>,
}

impl Table for EvidenceGwasCredibleSets {
    type Col = EvidenceGwasCredibleSetsCol;
    const NAME: &'static str = include_str!("../../sql/evidence/gwas_credible_sets.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(EvidenceGwasCredibleSetsCol {
    Id                     => "id"                     : Str,
    Score                  => "score"                  : F64,
    DiseaseName            => "diseaseName"            : Str,
    StudyLocusId           => "studyLocusId"           : Str,
    LeadVariantId          => "leadVariantId"          : Str,
    // Enum8 in ClickHouse, ordered by its numeric value.
    LeadChromosome         => "leadChromosome"         : I64,
    LeadPosition           => "leadPosition"           : I64,
    PValueMantissa         => "pValueMantissa"         : F64,
    PValueExponent         => "pValueExponent"         : I64,
    Beta                   => "beta"                   : F64,
    FinemappingMethod      => "finemappingMethod"      : Str,
    Confidence             => "confidence"             : Str,
    // Position of `confidence` in the fine-mapping confidence order, best highest.
    ConfidenceRank         => "confidenceRank"         : U64,
    StudyId                => "studyId"                : Str,
    TraitFromSource        => "traitFromSource"        : Str,
    PublicationFirstAuthor => "publicationFirstAuthor" : Str,
    PublicationDate        => "publicationDate"        : Str,
    NSamples               => "nSamples"               : U64,
});

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    scope: EvidenceScope,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(
        scope: EvidenceScope,
        sort: Vec<Sort<SortField>>,
        search: Option<Search>,
        index: u32,
        size: u32,
    ) -> Self {
        Self {
            scope,
            sort,
            search: search.and_then(Search::normalise),
            index,
            size,
        }
    }

    fn to_spec(&self) -> Result<PageSpec<EvidenceGwasCredibleSets>> {
        let mut filters = vec![
            self.scope
                .candidate_filter(EvidenceGwasCredibleSetsCol::Id, DATASOURCE)?,
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
#[graphql(name = "EvidenceGwasCredibleSetsSortField")]
pub enum SortField {
    /// Evidence score, which is the target's locus-to-gene (L2G) score.
    Score,
    /// Lead variant, by genomic position.
    LeadVariant,
    /// Number of samples in the study.
    SampleSize,
    /// P-value of the lead variant.
    PValue,
    /// Effect size of the lead variant.
    Beta,
    /// Fine-mapping confidence, from PICS on a reported top hit up to `SuSiE` with in-sample LD.
    FinemappingConfidence,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::Score,
    direction: SortDirection::Descending,
}];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<EvidenceGwasCredibleSetsCol>> {
        use EvidenceGwasCredibleSetsCol as C;
        let cols: &[C] = match self {
            Self::Score => &[C::Score],
            Self::LeadVariant => &[C::LeadChromosome, C::LeadPosition, C::LeadVariantId],
            Self::SampleSize => &[C::NSamples],
            Self::PValue => &[C::PValueExponent, C::PValueMantissa],
            Self::Beta => &[C::Beta],
            Self::FinemappingConfidence => &[C::ConfidenceRank],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against: the text shown in the table.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EvidenceGwasCredibleSetsSearchField")]
    pub enum SearchField => EvidenceGwasCredibleSetsCol {
        LeadVariantId,
        TraitFromSource,
        DiseaseName,
        StudyId,
        FinemappingMethod,
        Confidence,
        PublicationFirstAuthor,
        PublicationDate,
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
    evidences: Paged<EvidenceGwasCredibleSets>,
}

#[derive(From)]
pub struct EvidenceGwasCredibleSetsLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceGwasCredibleSetsLoader {
    type Value = Paged<EvidenceGwasCredibleSets>;
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
            .map(|row| (keys[row.key].clone(), row.evidences))
            .collect())
    }
}

/// # Errors
/// If the `ClickHouse` query fails.
pub async fn load_evidence_gwas_credible_sets(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceGwasCredibleSets>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceGwasCredibleSetsLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
