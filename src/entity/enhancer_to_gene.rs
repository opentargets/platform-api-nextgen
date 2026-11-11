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
    entity::{biosample, target, variant::Chromosome},
    query::{
        paginate::{Page, Paged},
        sort::{Sort, SortDirection},
        sql::{
            Column, Filter, Op, OrderBy, PageSpec, Table, Value, ValueKind, build, column_subset,
            columns,
        },
    },
};

// ---- models ----

///Score from a specific datasource.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ResourceScore {
    /// Name of the resource providing the score.
    name: String,
    /// Score value from the resource.
    value: f64,
}

///Regulatory enhancer/promoter regions to gene (target) predictions for
///a specific tissue/cell type based on the integration of experimental sources.
///
/// Field order must match the column list in `enhancer_to_gene.sql`.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct EnhancerToGene {
    /// Chromosome containing the regulatory region.
    chromosome: Chromosome,
    /// Genomic start position of the regulatory region.
    start: u32,
    /// Genomic end position of the regulatory region.
    end: u32,
    // foreign keys
    #[graphql(skip)]
    gene_id: String,
    /// Name of the biosample where the interval was identified.
    biosample_name: String,
    #[graphql(skip)]
    biosample_id: String,
    /// Identifier of the biosample defined by the datasource provider.
    biosample_from_source_id: Option<String>,
    /// Type of regulatory region (e.g., enhancer, promoter).
    interval_type: String,
    /// Distance from the regulatory region to the transcription start site.
    distance_to_tss: i32,
    /// Combined score for the enhancer/promoter region to gene prediction.
    score: f64,
    /// Scores from individual resources used in prediction.
    resource_score: Vec<ResourceScore>,
    /// Identifier of the data source providing the regulatory region to gene prediction.
    datasource_id: String,
    /// PubMed identifier for the study providing the evidence [bioregistry:pubmed].
    pmid: String,
    /// Identifier of the study providing the experimental data.
    study_id: String,
    /// Quality control flags for this interval.
    quality_controls: Vec<String>,
    // Joined in for sort and search only. The GraphQL `target` and `biosample`
    // fields serve this data.
    #[graphql(skip)]
    #[allow(dead_code, reason = "read only to keep the column order")]
    target_approved_symbol: Option<String>,
    #[graphql(skip)]
    #[allow(dead_code, reason = "read only to keep the column order")]
    biosample_display_name: Option<String>,
}

#[ComplexObject]
impl EnhancerToGene {
    ///Cell type or tissue where the regulatory region to gene prediction was identified.
    async fn biosample(&self, ctx: &Context<'_>) -> Result<Option<biosample::Biosample>> {
        biosample::load_biosample_by_id(ctx, self.biosample_id.clone()).await
    }
    /// Predicted gene (target).
    async fn target(&self, ctx: &Context<'_>) -> Result<Option<target::Target>> {
        target::load_target(ctx, self.gene_id.clone()).await
    }
}

impl Table for EnhancerToGene {
    type Col = E2gCol;
    const NAME: &'static str = include_str!("../sql/enhancer_to_gene.sql");
    // No single id column, but this combination is unique.
    const TIE_BREAKER: &'static str =
        "tuple(start, `end`, geneId, biosampleId, studyId, intervalType)";
}

columns!(E2gCol {
    // `chromosome` is an Enum8, compared by its number (e.g. 23 for X).
    Chromosome           => "chromosome"           : I64,
    Start                => "start"                : I64,
    // Backticks because `end` is an SQL keyword.
    End                  => "`end`"                : I64,
    TargetApprovedSymbol => "targetApprovedSymbol" : Str,
    BiosampleName        => "biosampleDisplayName" : Str,
    IntervalType         => "intervalType"         : Str,
    StudyId              => "studyId"              : Str,
    DistanceToTss        => "distanceToTss"        : I64,
    // Both ends are inclusive.
    IntervalSize         => "`end` - start + 1"    : I64,
    Score                => "score"                : F64,
});

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    chromosome: Chromosome,
    position: u32,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    fn to_spec(&self) -> Result<PageSpec<EnhancerToGene>> {
        use E2gCol as C;
        let pos = i64::from(self.position);
        // Regions that contain the position. Uses the table's (chromosome, start, end) key.
        let mut filters = vec![
            Filter::new(C::Chromosome, Op::Eq, Value::I64(self.chromosome as i64)),
            Filter::new(C::Start, Op::Le, Value::I64(pos)),
            Filter::new(C::End, Op::Ge, Value::I64(pos)),
        ]
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(Error::new)?;
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

/// Fields available for sorting enhancer-to-gene predictions.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum, Hash)]
#[graphql(name = "EnhancerToGeneSortField")]
pub enum SortField {
    /// Type of regulatory region, alphabetically.
    IntervalType,
    /// Distance from the regulatory region to the transcription start site.
    DistanceToTss,
    /// Length of the regulatory region in base pairs.
    IntervalSize,
    /// Combined prediction score.
    Score,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::Score,
    direction: SortDirection::Descending,
}];

impl SortField {
    /// Highest score first.
    #[must_use]
    pub fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<E2gCol>> {
        use E2gCol as C;
        let col = match self {
            Self::IntervalType => C::IntervalType,
            Self::DistanceToTss => C::DistanceToTss,
            Self::IntervalSize => C::IntervalSize,
            Self::Score => C::Score,
        };
        OrderBy::each(&[col], dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EnhancerToGeneSearchField")]
    pub enum SearchField => E2gCol {
        TargetApprovedSymbol,
        BiosampleName,
        IntervalType,
        StudyId,
    }
}

/// Case-insensitive text search over the `SearchField`s.
pub type Search = crate::query::search::Search<SearchField>;

// ---- loaders ----

/// One row of the batched query: the page of results for `keys[key]`.
#[derive(Debug, Clone, Row, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRow {
    key: usize,
    enhancer_to_genes: Paged<EnhancerToGene>,
}

/// Loads pages of enhancer-to-gene predictions, batching every lookup in one
/// request into a single query.
#[derive(From)]
pub struct EnhancerToGeneLoader {
    ch: ClickHouse,
}

impl EnhancerToGeneLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<QueryParams> for EnhancerToGeneLoader {
    type Value = Paged<EnhancerToGene>;
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
            .map(|row| (keys[row.key].clone(), row.enhancer_to_genes))
            .collect())
    }
}

/// Loads one page of the enhancer-to-gene predictions whose region contains
/// `position` on `chromosome`.
///
/// # Errors
/// If the `ClickHouse` query fails.
pub async fn load_enhancer_to_genes(
    ctx: &Context<'_>,
    chromosome: Chromosome,
    position: u32,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    page: Page,
) -> Result<Paged<EnhancerToGene>> {
    let key = QueryParams {
        chromosome,
        position,
        sort,
        search: search.and_then(Search::normalise),
        index: page.index,
        size: page.size,
    };
    Ok(ctx
        .data_unchecked::<DataLoader<EnhancerToGeneLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
