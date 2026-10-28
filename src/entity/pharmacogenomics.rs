//! Pharmacogenomics records for a target, variant or drug.
//!
//! Each parent entity holds `pharmacogenomicsIds`, the keys of its rows in the
//! `pharmacogenomics` table. Sort, search and paging run in `ClickHouse` over
//! those rows, with the target symbol and consequence label joined in on the fly
//! (see the [`Table`] impl) so they can be sorted and searched too.

use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, Enum, Error, InputObject, Result, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;
use strum::{EnumIter, IntoEnumIterator};

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        drug::{Drug, load_drug},
        sequence_ontology::{SequenceOntologyTerm, load_sequence_ontology_one},
        target::{Target, load_target},
    },
    query::{
        paginate::{Page, Paged},
        sort::{Sort, SortDirection},
        sql::{Column, Filter, OrderBy, PageSpec, Table, ValueKind, build, column_subset, columns},
    },
};

// ---- models ----

/// Drug with drug identifiers.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct DrugWithIdentifiers {
    /// Drug or clinical candidate identifier.
    drug_id: String,
    /// Drug identifier from the original data source.
    drug_from_source: String,
}

/// Genetic variants influencing individual drug responses. Pharmacogenetics data is integrated from
/// sources including Pharmacogenomics Knowledgebase (`PharmGKB`).
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct VariantAnnotation {
    /// Allele or genotype in the base case.
    base_allele_or_genotype: Option<String>,
    /// Allele or genotype in the comparison case.
    comparison_allele_or_genotype: Option<String>,
    /// Indicates in which direction the genetic variant increases or decreases drug response.
    directionality: Option<String>,
    /// Allele observed effect.
    effect: Option<String>,
    /// Summary of the impact of the allele on the drug response.
    effect_description: Option<String>,
    /// Type of effect.
    effect_type: Option<String>,
    /// Entity affected by the effect.
    entity: Option<String>,
    /// PubMed identifier (PMID) of the literature entry.
    literature: Option<String>,
}

/// Pharmacogenomics data linking genetic variants to drug responses. Data is integrated from
/// sources including ClinPGx.
///
/// Field order must match `Table::SELECT` below: rows are read positionally.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Pharmacogenomics {
    /// Row key, used only to keep paging stable.
    #[graphql(skip)]
    #[allow(dead_code, reason = "read only to keep the column order")]
    row_id: u64,
    /// Identifier for the data provider.
    datasource_id: String,
    /// Classification of the type of pharmacogenomic data (e.g., `clinical_annotation`).
    datatype_id: String,
    /// List of drugs or clinical candidates associated with the pharmacogenomic data.
    drugs: Vec<DrugWithIdentifiers>,
    /// Strength of the scientific support for the variant/drug response.
    evidence_level: String,
    /// Genetic variant configuration.
    genotype: Option<String>,
    /// Explanation of the genotype's clinical significance.
    genotype_annotation_text: Option<String>,
    /// Identifier for the specific genetic variant combination (e.g., `1_1500_A_A,T`).
    genotype_id: Option<String>,
    /// Haplotype ID in the ClinPGx dataset.
    haplotype_from_source_id: Option<String>,
    /// Combination of genetic variants that constitute a particular allele of a gene (e.g.,
    /// CYP2C9*3).
    haplotype_id: Option<String>,
    /// PubMed identifier (PMID) of the literature entry [bioregistry:pubmed].
    literature: Vec<String>,
    /// Classification of the drug response type (e.g., Toxicity).
    pgx_category: String,
    /// Phenotype identifier from the source.
    phenotype_from_source_id: Option<String>,
    /// Description of the phenotype associated with the variant.
    phenotype_text: Option<String>,
    /// Annotation details about the variant effect on drug response.
    variant_annotation: Vec<VariantAnnotation>,
    /// Identifier of the study providing the pharmacogenomic evidence.
    study_id: Option<String>,
    /// Target (gene/protein) identifier as reported by the data source.
    target_from_source_id: Option<String>,
    /// The sequence ontology identifier of the consequence of the variant based on Ensembl VEP in
    /// the context of the transcript [bioregistry:so].
    variant_functional_consequence_id: Option<String>,
    /// dbSNP rsID identifier for the variant.
    variant_rs_id: Option<String>,
    /// Variant identifier in CHROM_POS_REF_ALT notation.
    variant_id: Option<String>,
    /// Whether the target is directly affected by the variant.
    is_direct_target: bool,
    // Joined in for sort and search only. The GraphQL `target`,
    // `variantFunctionalConsequence` and `drugs` fields serve this data.
    #[graphql(skip)]
    #[allow(dead_code, reason = "read only to keep the column order")]
    target_approved_symbol: Option<String>,
    #[graphql(skip)]
    #[allow(dead_code, reason = "read only to keep the column order")]
    variant_functional_consequence_label: Option<String>,
    #[graphql(skip)]
    #[allow(dead_code, reason = "read only to keep the column order")]
    drug_ids: Vec<String>,
    #[graphql(skip)]
    #[allow(dead_code, reason = "read only to keep the column order")]
    drug_from_sources: Vec<String>,
}

#[ComplexObject]
impl Pharmacogenomics {
    /// Target entity.
    async fn target(&self, ctx: &Context<'_>) -> Result<Option<Target>> {
        match &self.target_from_source_id {
            Some(target_id) => load_target(ctx, target_id.clone()).await,
            None => Ok(None),
        }
    }

    /// The sequence ontology identifier of the consequence of the variant based on Ensembl VEP in
    /// the context of the transcript [bioregistry:so]
    async fn variant_functional_consequence(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<SequenceOntologyTerm>> {
        match &self.variant_functional_consequence_id {
            Some(id) => load_sequence_ontology_one(ctx, id.replace('_', ":")).await,
            None => Ok(None),
        }
    }
}

#[ComplexObject]
impl DrugWithIdentifiers {
    ///Drug or clinical candidate entity
    async fn drug(&self, ctx: &Context<'_>) -> Result<Option<Drug>> {
        load_drug(ctx, self.drug_id.clone()).await
    }
}

impl Table for Pharmacogenomics {
    type Col = PgxCol;
    const NAME: &'static str = "pharmacogenomics AS p";
    const TIE_BREAKER: &'static str = "rowId";
    // Every column, plus the target symbol and consequence label so they can be
    // sorted and searched. Order must match the struct. `nullIf` turns unmatched
    // joins ('') into NULL so they sort last.
    const SELECT: &'static str = r"
        p.rowId AS rowId,
        p.datasourceId AS datasourceId,
        p.datatypeId AS datatypeId,
        p.drugs AS drugs,
        p.evidenceLevel AS evidenceLevel,
        p.genotype AS genotype,
        p.genotypeAnnotationText AS genotypeAnnotationText,
        p.genotypeId AS genotypeId,
        p.haplotypeFromSourceId AS haplotypeFromSourceId,
        p.haplotypeId AS haplotypeId,
        p.literature AS literature,
        p.pgxCategory AS pgxCategory,
        p.phenotypeFromSourceId AS phenotypeFromSourceId,
        p.phenotypeText AS phenotypeText,
        p.variantAnnotation AS variantAnnotation,
        p.studyId AS studyId,
        p.targetFromSourceId AS targetFromSourceId,
        p.variantFunctionalConsequenceId AS variantFunctionalConsequenceId,
        p.variantRsId AS variantRsId,
        p.variantId AS variantId,
        p.isDirectTarget AS isDirectTarget,
        if(p.targetFromSourceId IS NULL, NULL, nullIf(t.approvedSymbol, ''))
            AS targetApprovedSymbol,
        if(p.variantFunctionalConsequenceId IS NULL, NULL, nullIf(toString(so.label), ''))
            AS variantFunctionalConsequenceLabel,
        arrayMap(d -> toString(d.drugId), p.drugs) AS drugIds,
        arrayMap(d -> toString(d.drugFromSource), p.drugs) AS drugFromSources";
    const JOINS: &'static str = r"
        LEFT JOIN targets AS t ON ifNull(p.targetFromSourceId, '') = t.id
        LEFT JOIN sequence_ontology AS so
            ON toLowCardinality(replaceOne(ifNull(p.variantFunctionalConsequenceId, ''), '_', ':'))
                = so.id";
}

columns!(PgxCol {
    RowId                             => "rowId"                             : U64,
    VariantRsId                       => "variantRsId"                       : Str,
    GenotypeId                        => "genotypeId"                        : Str,
    HaplotypeId                       => "haplotypeId"                       : Str,
    HaplotypeFromSourceId             => "haplotypeFromSourceId"             : Str,
    TargetFromSourceId                => "targetFromSourceId"                : Str,
    TargetApprovedSymbol              => "targetApprovedSymbol"              : Str,
    VariantFunctionalConsequenceLabel => "variantFunctionalConsequenceLabel" : Str,
    DrugIds                           => "drugIds"                           : StrArray,
    DrugFromSources                   => "drugFromSources"                   : StrArray,
    PhenotypeFromSourceId             => "phenotypeFromSourceId"             : Str,
    GenotypeAnnotationText            => "genotypeAnnotationText"            : Str,
    PhenotypeText                     => "phenotypeText"                     : Str,
    PgxCategory                       => "pgxCategory"                       : Str,
    EvidenceLevel                     => "evidenceLevel"                     : Str,
    StudyId                           => "studyId"                           : Str,
});

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    /// Rows to choose from: the parent entity's `pharmacogenomicsIds`.
    ids: Vec<u64>,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    fn to_spec(&self) -> Result<PageSpec<Pharmacogenomics>> {
        let mut filters =
            vec![Filter::in_u64(PgxCol::RowId, self.ids.clone()).map_err(Error::new)?];
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

/// Fields available for sorting pharmacogenomics records.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum, Hash)]
#[graphql(name = "PharmacogenomicsSortField")]
pub enum SortField {
    /// Evidence level, strongest (1A) first when ascending.
    EvidenceLevel,
    PgxCategory,
    /// Target approved symbol.
    Target,
    /// Drug names from the source.
    Drug,
    /// Variant rsID.
    VariantRsId,
    GenotypeId,
    /// Variant functional consequence label.
    VariantConsequence,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::EvidenceLevel,
    direction: SortDirection::Ascending,
}];

impl SortField {
    /// Strongest evidence first.
    #[must_use]
    pub fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<PgxCol>> {
        use PgxCol as C;
        let col = match self {
            Self::EvidenceLevel => C::EvidenceLevel,
            Self::PgxCategory => C::PgxCategory,
            Self::Target => C::TargetApprovedSymbol,
            Self::Drug => C::DrugFromSources,
            Self::VariantRsId => C::VariantRsId,
            Self::GenotypeId => C::GenotypeId,
            Self::VariantConsequence => C::VariantFunctionalConsequenceLabel,
        };
        OrderBy::each(&[col], dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "PharmacogenomicsSearchField")]
    pub enum SearchField => PgxCol {
        VariantRsId,
        GenotypeId,
        HaplotypeId,
        HaplotypeFromSourceId,
        TargetFromSourceId,
        TargetApprovedSymbol,
        VariantFunctionalConsequenceLabel,
        DrugIds,
        DrugFromSources,
        PhenotypeFromSourceId,
        GenotypeAnnotationText,
        PhenotypeText,
        PgxCategory,
        EvidenceLevel,
        StudyId,
    }
}

impl SearchField {
    /// Every searchable field; the default when the client doesn't pick any.
    fn all() -> Vec<Self> { Self::iter().collect() }
}

/// Case-insensitive text search over pharmacogenomics records.
#[derive(Debug, Clone, Eq, PartialEq, Hash, InputObject)]
#[graphql(name = "PharmacogenomicsSearch")]
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

    fn to_filter(&self) -> Result<Filter<PgxCol>> {
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
    pharmacogenomics: Paged<Pharmacogenomics>,
}

/// Loads pages of pharmacogenomics records, batching the targets, variants and
/// drugs in one request into a single query.
#[derive(From)]
pub struct PharmacogenomicsLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for PharmacogenomicsLoader {
    type Value = Paged<Pharmacogenomics>;
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
            .map(|row| (keys[row.key].clone(), row.pharmacogenomics))
            .collect())
    }
}

/// Loads one page of the pharmacogenomics records with the given row ids.
///
/// # Errors
/// If the `ClickHouse` query fails.
pub async fn load_pharmacogenomics(
    ctx: &Context<'_>,
    ids: &[u64],
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    page: Page,
) -> Result<Paged<Pharmacogenomics>> {
    if ids.is_empty() {
        return Ok(Paged::default());
    }
    let key = QueryParams {
        ids: ids.to_vec(),
        sort,
        search: search.and_then(Search::normalise),
        index: page.index,
        size: page.size,
    };
    Ok(ctx
        .data_unchecked::<DataLoader<PharmacogenomicsLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
