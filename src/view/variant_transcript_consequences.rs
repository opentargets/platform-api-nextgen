use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, Enum, Error, Result, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;
use strum::EnumIter;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        sequence_ontology::{SequenceOntologyTerm, load_sequence_ontology_many},
        target::{Target, load_target},
    },
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

/// Predicted consequence of a variant on one transcript.
///
/// Field order must match the column list in `variant_transcript_consequences.sql`.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct VariantTranscriptConsequence {
    /// Variant identifier in CHROM_POS_REF_ALT notation.
    variant_id: String,
    /// The sequence ontology identifiers of the consequence of the variant based on Ensembl VEP
    /// in the context of the transcript [bioregistry:so].
    #[graphql(skip)]
    variant_functional_consequence_ids: Vec<String>,
    /// Amino acid change caused by this variant on this gene.
    amino_acid_change: Option<String>,
    /// Uniprot identifiers of the gene product [bioregistry:uniprot].
    uniprot_accessions: Vec<String>,
    /// Flagging if the transcript is the canonical transcript for the gene.
    is_ensembl_canonical: bool,
    /// Affected codon in the transcript.
    codons: Option<String>,
    /// Distance of the variant from the transcript.
    distance_from_footprint: i32,
    /// Distance of the variant from the transcription start site.
    distance_from_tss: i32,
    /// Open Targets target identifier of the transcript [bioregistry:ensembl].
    #[graphql(skip)]
    target_id: Option<String>,
    /// Ensembl VEP predicted impact of the variant on the transcript.
    impact: Option<String>,
    /// Ensembl transcript identifier [bioregistry:ensembl].
    transcript_id: Option<String>,
    /// Loss-of-function prediction based on LOFTEE.
    loftee_prediction: Option<String>,
    /// Index of the transcript in the list of transcripts around the gene.
    transcript_index: u32,
    /// Score assigned to transcript based on Ensembl VEP consequence.
    consequence_score: f64,
    // Joined in for sort and search only. The GraphQL `target` and
    // `variantConsequences` fields serve this data.
    #[graphql(skip)]
    #[allow(dead_code, reason = "read only to keep the column order")]
    target_approved_symbol: Option<String>,
    #[graphql(skip)]
    #[allow(dead_code, reason = "read only to keep the column order")]
    variant_consequence_labels: Vec<String>,
}

#[ComplexObject]
impl VariantTranscriptConsequence {
    /// The target (gene/protein) associated with the transcript.
    async fn target(&self, ctx: &Context<'_>) -> Result<Option<Target>> {
        match &self.target_id {
            Some(target_id) => load_target(ctx, target_id.clone()).await,
            None => Ok(None),
        }
    }

    /// The sequence ontology terms of the consequence of the variant based on Ensembl VEP in the
    /// context of the transcript.
    async fn variant_consequences(&self, ctx: &Context<'_>) -> Result<Vec<SequenceOntologyTerm>> {
        load_sequence_ontology_many(ctx, &self.variant_functional_consequence_ids).await
    }
}

impl Table for VariantTranscriptConsequence {
    type Col = VariantTranscriptConsequenceCol;
    const NAME: &'static str = include_str!("../sql/variant_transcript_consequences.sql");
    // Unique within a variant, and every query is for one variant.
    const TIE_BREAKER: &'static str = "transcriptIndex";
}

columns!(VariantTranscriptConsequenceCol {
    VariantId                => "variantId"                : Str,
    TargetApprovedSymbol     => "targetApprovedSymbol"     : Str,
    // Labels are stored as e.g. `intron_variant`; search them as shown, `intron variant`.
    VariantConsequenceLabels => "arrayMap(l -> replaceAll(l, '_', ' '), variantConsequenceLabels)" : StrArray,
    Impact                   => "impact"                   : Str,
    DistanceFromFootprint    => "distanceFromFootprint"    : I64,
    DistanceFromTss          => "distanceFromTss"          : I64,
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

    fn to_spec(&self) -> Result<PageSpec<VariantTranscriptConsequence>> {
        let mut filters = vec![
            Filter::new(
                VariantTranscriptConsequenceCol::VariantId,
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
#[graphql(name = "VariantTranscriptConsequencesSortField")]
pub enum SortField {
    /// Approved gene symbol of the target. Rows without a target come last.
    TargetApprovedSymbol,
    /// Distance of the variant from the transcript.
    DistanceFromFootprint,
    /// Distance of the variant from the transcription start site.
    DistanceFromTss,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::DistanceFromFootprint,
    direction: SortDirection::Ascending,
}];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<VariantTranscriptConsequenceCol>> {
        use VariantTranscriptConsequenceCol as C;
        let col = match self {
            Self::TargetApprovedSymbol => C::TargetApprovedSymbol,
            Self::DistanceFromFootprint => C::DistanceFromFootprint,
            Self::DistanceFromTss => C::DistanceFromTss,
        };
        OrderBy::each(&[col], dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "VariantTranscriptConsequencesSearchField")]
    pub enum SearchField => VariantTranscriptConsequenceCol {
        TargetApprovedSymbol,
        VariantConsequenceLabels,
        Impact,
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
    transcript_consequences: Paged<VariantTranscriptConsequence>,
}

#[derive(From)]
pub struct VariantTranscriptConsequencesLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for VariantTranscriptConsequencesLoader {
    type Value = Paged<VariantTranscriptConsequence>;
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
            .map(|row| (keys[row.key].clone(), row.transcript_consequences))
            .collect())
    }
}

/// # Errors
/// If the `ClickHouse` query fails.
pub async fn load_variant_transcript_consequences(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<VariantTranscriptConsequence>> {
    Ok(ctx
        .data_unchecked::<DataLoader<VariantTranscriptConsequencesLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
