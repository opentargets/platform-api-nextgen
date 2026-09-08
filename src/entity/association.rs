use std::{collections::HashSet, f64::consts::PI, marker::PhantomData};

use async_graphql::{ComplexObject, Context, Enum, InputObject, OutputType, SimpleObject};
use clickhouse::Row;
use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoEnumIterator, IntoStaticStr};
use tracing::instrument;

use crate::{
    datasource::{clickhouse::ClickHouse, opensearch::OpenSearch},
    entity::{
        disease::{Disease, load_disease},
        search_facet::facet_entity_ids,
        target::{Target, load_target},
    },
    query::{
        paginate::{Page, Paged},
        sort::SortDirection,
    },
};

// ---- constants ----

/// Precomputed harmonic series constant for scoring. See Basel Problem.
/// Note: this was `1.644_924_066_898_242_3` in the old Scala API, because the approximation method
/// was not precise enough. This makes the scores differ in their 5th decimal.
// const MAX_HS: f64 = 1.644_924_066_898_242_3;
const MAX_HS: f64 = PI * PI / 6.0;
/// The weight of indirect associations.
const INDIRECT_WEIGHT: f64 = 0.5;

// ---- helpers ----

/// Escape for a single-quoted ClickHouse literal.
fn esc(s: &str) -> String { s.replace('\\', "\\\\").replace('\'', "\\'") }

/// Quote a set of strings for use in a ClickHouse IN clause.
fn quoted_set<S: AsRef<str>>(items: &[S]) -> String {
    items
        .iter()
        .map(|s| format!("'{}'", esc(s.as_ref())))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---- models ----

// ** The `Datasource` enum **
// * Contains the data sources for evidences.

/// Represents a datasource for association scoring.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Hash, Enum, EnumIter, IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[graphql(rename_items = "snake_case")]
pub enum Datasource {
    /// Clinical evidence linking a target/disease via a drug that targets the gene product and is
    /// indicated for the disease (approved or in development); inferred from clinical reports +
    /// drug MoA data.
    /// See <https://platform-docs.opentargets.org/evidence#clinical-precedence>
    ClinicalPrecedence,
    /// Target-disease relationships from GWAS-significant signals, fine-mapped, colocalised against
    /// molQTL, and scored by the Locus-to-Gene (L2G) ML method; evidence is any credible set with
    /// L2G > 0.05.
    /// See <https://platform-docs.opentargets.org/evidence#gwas-associations>
    GwasCredibleSets,
    /// Gene–phenotype relationships observed in gene-level association tests using rare variant
    /// collapsing analyses.
    /// See <https://platform-docs.opentargets.org/evidence#gene-burden>
    GeneBurden,
    /// Germline variant-phenotype relationships from ClinVar (NIH archive), the germline subset;
    /// each evidence captures a single RCV record.
    /// See <https://platform-docs.opentargets.org/evidence#clinvar>
    Eva,
    /// Gene-disease relationships from Genomics England PanelApp expert-reviewed gene panels.
    /// See <https://platform-docs.opentargets.org/evidence#genomics-england-gel-panelapp>
    GenomicsEngland,
    /// Gene-disease relationships from Gene2Phenotype (G2P), literature-curated by expert clinical
    /// geneticist panels.
    /// See <https://platform-docs.opentargets.org/evidence#gene2phenotype>
    #[strum(serialize = "gene2phenotype")]
    #[graphql(name = "gene2phenotype")]
    Gene2Phenotype,
    /// UniProt-curated target-disease relationships from publications supporting a protein's
    /// involvement in disease, aggregated per evidence.
    /// See <https://platform-docs.opentargets.org/evidence#uniprot-literature>
    UniprotLiterature,
    /// UniProt-curated variants known to alter protein function in disease, aggregated from
    /// supporting publications.
    /// See <https://platform-docs.opentargets.org/evidence#uniprot-curated-variants>
    UniprotVariants,
    /// Orphanet gene-disease associations for rare disorders of genetic origin, with relationship
    /// classification, mutation type, and supporting references.
    /// See <https://platform-docs.opentargets.org/evidence#orphanet>
    Orphanet,
    /// ClinGen gene-disease validity curation: evaluates the strength of evidence for a gene
    /// causing a disease, classified via a semi-quantitative framework.
    /// See <https://platform-docs.opentargets.org/evidence#clingen>
    Clingen,
    /// Cancer Gene Census (CGC, part of COSMIC): curated genes with mutations causally implicated
    /// in cancer, aggregated per target-disease.
    /// See <https://platform-docs.opentargets.org/evidence#cancer-gene-census>
    CancerGeneCensus,
    /// IntOGen consensus cancer driver genes from harmonised tumour sequencing (PCAWG and others);
    /// each evidence is a significant driver in one cohort.
    /// See <https://platform-docs.opentargets.org/evidence#intogen>
    Intogen,
    /// Somatic variant-phenotype relationships from ClinVar (NIH archive), the somatic subset; each
    /// evidence captures a single RCV record.
    /// See <https://platform-docs.opentargets.org/evidence#clinvar-somatic>
    EvaSomatic,
    /// Expert-curated cancer biomarkers of drug sensitivity, resistance, and toxicity from Cancer
    /// Genome Interpreter, by cancer type.
    /// See <https://platform-docs.opentargets.org/evidence#cancer-biomarkers>
    CancerBiomarkers,
    /// Target-disease evidence from genome-wide CRISPRi/a/KO functional genomics screens in human
    /// brain cell types (CRISPRbrain), linking cell types to diseases.
    /// See <https://platform-docs.opentargets.org/evidence#crispr-screens>
    CrisprScreen,
    /// Cancer target dependencies from whole-genome CRISPR-Cas9 fitness screens in cell lines
    /// (Project Score, Sanger), mapped to tumours; targets scoring ≥ 36.0.
    /// See <https://platform-docs.opentargets.org/evidence#project-score>
    Crispr,
    /// Reactome-curated reaction pathways affected by disease, linking target to disease via
    /// protein-coding mutation or altered expression.
    /// See <https://platform-docs.opentargets.org/evidence#reactome>
    Reactome,
    /// Target-disease co-occurrences mined from Europe PMC literature via deep-learning NER,
    /// aggregated per publication with a confidence assessment.
    /// See <https://platform-docs.opentargets.org/evidence#europe-pmc>
    Europepmc,
    /// Target-disease evidence from differentially expressed genes (disease vs control) in EMBL-EBI
    /// Expression Atlas; each study contrast is one evidence.
    /// See <https://platform-docs.opentargets.org/evidence#expression-atlas>
    ExpressionAtlas,
    /// Target-disease evidence from mouse knockout genotype-phenotype associations (IMPC), scored
    /// by human-mouse phenotypic similarity (PhenoDigm).
    /// See <https://platform-docs.opentargets.org/evidence#impc>
    Impc,
    OtCrisprValidation,
    OtCrispr,
    Encore,
}

impl AsRef<str> for Datasource {
    fn as_ref(&self) -> &str { (*self).into() }
}

// ** The `DatasourcePolicy` models **
// * Represent the query settings for every Datasource

/// Represents the policy for a datasource.
#[derive(Debug, InputObject, Clone, Copy)]
struct DatasourcePolicy {
    /// The weight of the datasource in association scoring. Range is [0.0, 1.0].
    weight: f64,
    /// Whether the datasource is required for the association to be considered valid.
    required: bool,
}

impl Default for DatasourcePolicy {
    #[rustfmt::skip]
    fn default() -> Self { Self { weight: 1.0, required: false } }
}

impl Datasource {
    /// Returns the default policy for the datasource.
    fn default_policy(self) -> DatasourcePolicy {
        let d = DatasourcePolicy::default();
        match self {
            Self::CancerBiomarkers | Self::OtCrisprValidation | Self::OtCrispr | Self::Encore => {
                DatasourcePolicy { weight: 0.5, ..d }
            }
            Self::Europepmc | Self::ExpressionAtlas | Self::Impc => {
                DatasourcePolicy { weight: 0.2, ..d }
            }
            _ => d,
        }
    }
}

/// Policy override for a datasource.
#[derive(Debug, InputObject)]
pub struct DatasourcePolicyOverride {
    /// The datasource to override the policy for.
    id: Datasource,
    /// The policy to override with.
    policy: DatasourcePolicy,
}

/// A list of tuples `Datasource`, `DatasourcePolicy` to use in a query.
struct DatasourcePolicies(Vec<(Datasource, DatasourcePolicy)>);

impl DatasourcePolicies {
    /// Creates a `DatasourcePolicies` from a list of `DatasourcePolicyOverride`s.
    fn from_overrides(overrides: &[DatasourcePolicyOverride]) -> Self {
        Datasource::iter()
            .map(|ds| {
                let p = overrides
                    .iter()
                    .find(|o| o.id == ds)
                    .map_or_else(|| ds.default_policy(), |o| o.policy);
                (ds, p)
            })
            .collect()
    }

    // Render methods: Helpers to put this into a SQL query.

    /// Returns the Datasources as a string of the form `ds1, ds2, ...`.
    fn render_datasources(&self) -> String {
        self.0
            .iter()
            .map(|(ds, _)| format!("'{}'", esc((*ds).into())))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Returns Datasource weights into a string of the form `weight1, weight2, ...`.
    fn render_weights(&self) -> String {
        self.0
            .iter()
            .map(|(_, p)| format!("{:?}", p.weight))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Return datasource requirement policies as a string of the form `required1, required2, ...`.
    fn render_requirements(&self) -> String {
        self.0
            .iter()
            .filter(|(_, p)| p.required)
            .map(|(ds, _)| format!("'{}'", esc((*ds).into())))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl FromIterator<(Datasource, DatasourcePolicy)> for DatasourcePolicies {
    /// Creates a `DatasourcePolicies` from an iterator of `(Datasource, DatasourcePolicy)` pairs.
    fn from_iter<T: IntoIterator<Item = (Datasource, DatasourcePolicy)>>(iter: T) -> Self {
        DatasourcePolicies(iter.into_iter().collect())
    }
}

/// Arguments for association queries.
///
/// The order of selection and filtering of the B dimension uses three arguments and happens in the
/// following stages:
///
/// A. First selection is made from `bs` and `facet_filters` together.
///      - `bs` empty, `facet_filters` empty     => all B entities.
///      - `bs` present, `facet_filters` empty   => the ids in `bs`.
///      - `bs` empty, `facet_filters` present   => the ids resolved from `facet_filters`.
///      - `bs` present, `facet_filters` present => their intersection.
/// B. `b_filter`: free-text filter applied on top of the set from (A).
#[derive(Debug)]
pub struct AssociationArguments {
    /// List of disease or target ids to use as the second dimension items for associations.
    pub bs: Vec<String>,
    /// Filter to apply to the B dimension items.
    pub b_filter: Option<String>,
    /// List of the facet IDs to filter the B dimension items by.
    pub facet_filters: Vec<String>,
    /// Whether to include diseases from the _measurement_ family in `B` set. Default is `false`.
    pub include_measurements: Option<bool>,
    /// List of datasource policy overrides.
    pub datasource_policy_overrides: Vec<DatasourcePolicyOverride>,
    /// Ordering for the associations.
    pub sort: AssociationSort,
    /// Pagination for the associations.
    pub page: Page,
}

// ** Other query argument models **
// * Represent other query settings exposed on GraphQL.

/// Sort types. Contain the sort field and direction.
#[derive(Debug, Clone, InputObject)]
pub struct AssociationSort {
    /// The key to sort by. Can either be `score` to use the overall association score (default) or
    /// a datasource id (e.g., `impc`).
    #[graphql(default = "score")]
    key: String,
    /// The direction to sort in.
    direction: SortDirection,
}

impl Default for AssociationSort {
    fn default() -> Self {
        Self {
            key: "score".into(),
            direction: SortDirection::Descending,
        }
    }
}

impl AssociationSort {
    fn render_sort_by(&self) -> String {
        match self.key.as_str() {
            "score" => "score".into(),
            "novelty" => "novelty".into(),
            _ => "score_indexed".into(),
        }
    }
    fn render_sort_datasource(&self) -> &'static str {
        Datasource::iter()
            .find(|d| <&str>::from(*d) == self.key)
            .map(<&str>::from)
            .unwrap_or_default()
    }
    fn render_direction(&self) -> &'static str {
        match self.direction {
            SortDirection::Ascending => "ASC",
            SortDirection::Descending => "DESC",
        }
    }
}

// ** The results models **
// * Represent the results of association queries.

/// A score for a datasource, used in association scoring.
#[derive(Debug, Clone, SimpleObject)]
pub struct Score {
    /// Identifier of the Datasource (e.g., `impc`, `chembl`).
    id: String,
    /// Association score for the Datasource. Scores are normalized to a range of 0-1. The higher
    /// the score, the stronger the association.
    score: f64,
}

/// A scored association between a disease and a target or vice versa.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(complex)]
#[graphql(concrete(name = "DiseaseAssociation", params(Disease)))]
#[graphql(concrete(name = "TargetAssociation", params(Target)))]
pub struct Association<T: OutputType + 'static> {
    #[graphql(skip)]
    id: String,
    /// Overall association score aggregated across all evidence types. A higher score indicates a
    /// stronger association between target and disease. Scores are normalized to a range of 0-1.
    score: f64,
    /// Association scores computed for every datasource (e.g., IMPC, ChEMBL, Gene2Phenotype).
    datasource_scores: Vec<Score>,
    /// A measure of how novel the target–disease association is, calculated based on the
    /// accumulation of direct evidence over time.
    novelty: Option<f64>,
    // marker for the actual embedded type (Disease or Target)
    #[graphql(skip)]
    _marker: PhantomData<T>,
}

// The concrete association types
/// Represents a scored association between a `Disease` and a `Target`.
pub type DiseaseAssociation = Association<Disease>;
/// Represents a scored association between a `Target` and a `Disease`.
pub type TargetAssociation = Association<Target>;

/// A ClickHouse row representing an association, result from the AOTF query.
///
/// Internal, not exposed on GraphQL.
#[derive(Row, Deserialize)]
struct AssociationRow {
    #[serde(rename = "B")]
    id: String,
    score: f64,
    datasource_scores: Vec<(String, f64)>,
    novelty: Option<f64>,
    total: u64,
}

impl AssociationRow {
    #[instrument(skip_all, level = "trace", fields(id = %self.id, score = %self.score))]
    // Converts an internal `AssociationRow` into a `Association`.
    fn into_association<T: OutputType + 'static>(self) -> Association<T> {
        let map = |v: Vec<(String, f64)>| {
            v.into_iter()
                .map(|(id, score)| Score { id, score })
                .collect()
        };
        Association {
            id: self.id,
            score: self.score,
            datasource_scores: map(self.datasource_scores),
            novelty: self.novelty,
            _marker: PhantomData,
        }
    }
}

// ** The SQL query models.
// * Represent an association query and all of the required data and traits needed for them.

/// A trait for entities that have associations.
pub trait EntityWithAssociations {
    /// The name of the association table.
    const TABLE: &'static str;
    /// The type of the B id for this entity's associations.
    type B: OutputType + 'static;
    /// Generate the ID set to use this entity as A in an association query.
    #[allow(async_fn_in_trait)]
    async fn a_ids(ch: &ClickHouse, anchor: &str) -> async_graphql::Result<Vec<String>>;
}

/// Contains the strings and other data needed to build an associations sql query.
pub struct AotfSql {
    table: &'static str,
    anchor: String,
    a_ids: Vec<String>,
    b_ids: Vec<String>,
    b_filter: Option<String>,
    policies: DatasourcePolicies,
    include_measurements: Option<bool>,
    sort: AssociationSort,
    page: Page,
}

impl AotfSql {
    /// Returns the WHERE clause for the association query.
    ///
    /// This is what compiles the set of association rows that will make it to the score calculation
    /// for each `Datasource` in the query.
    fn render_where(&self) -> String {
        let a_set = quoted_set(
            &std::iter::once(self.anchor.as_str())
                .chain(self.a_ids.iter().map(String::as_str))
                .collect::<Vec<_>>(),
        );

        // `anchor` term: keep the row if its `A` is in our `a_set`.
        let mut conj = vec![format!("A IN ({a_set})")];

        // `b_filter` term: match the rolled-up bucket's name, not the raw row's.
        if let Some(filter) = &self.b_filter {
            let terms: Vec<String> = filter
                .split_whitespace()
                .map(str::to_lowercase)
                .map(|t| format!("lower(name) LIKE '%{}%'", esc(&t)))
                .collect();
            if !terms.is_empty() {
                conj.push(format!(
                    "b_indirect IN (SELECT id FROM disease WHERE {})",
                    terms.join(" AND ")
                ));
            }
        }

        // `b_ids` term: keep the row if its `b_indirect` is in our `b_ids` set.
        if !self.b_ids.is_empty() {
            conj.push(format!("b_indirect IN ({})", quoted_set(&self.b_ids)));
        }

        // `include_measurements` term: keep the row unless it is a measurement and the policy is
        // explicitly set to `false`.
        if self.include_measurements == Some(false) {
            conj.push("isMeasurement = false".into());
        }

        // Join it all with AND, so all conditions must be met for the row to be included.
        conj.join(" AND ")
    }

    /// Returns the HAVING clause for the association query.
    ///
    /// This clause filters the results to only include rows that have evidence from at least one
    /// required datasource.
    fn render_having(&self) -> String {
        let requirements = &self.policies.render_requirements();
        if requirements.is_empty() {
            return String::new();
        }
        // groupArray(datasourceId) at `GROUP BY B` = every datasource the bucket
        // holds post-roll-up. hasAny = "has ≥1 of required" (matches old `IN`).
        format!("HAVING hasAny(groupArray(datasourceId), [{requirements}])")
    }

    #[must_use]
    fn build_query(&self) -> String {
        format!(
            include_str!("associations.sql"),
            max_hs = MAX_HS,
            indirect_w = INDIRECT_WEIGHT,
            novelty = "noveltyDirect",
            a_id = self.anchor,
            table = self.table,
            datasources = self.policies.render_datasources(),
            weights = self.policies.render_weights(),
            _where = self.render_where(),
            _having = self.render_having(),
            sort_by = self.sort.render_sort_by(),
            sort_datasource = self.sort.render_sort_datasource(),
            sort_direction = self.sort.render_direction(),
            offset = self.page.index * self.page.size,
            size = self.page.size,
        )
    }
}

// ---- loaders ----

/// Resolve the B ids for the associations query.
#[instrument(skip_all, level = "trace", fields(bs = ?args.bs, facet_filters = ?args.facet_filters))]
async fn prepare_b_ids(
    os: &OpenSearch,
    args: &AssociationArguments,
) -> async_graphql::Result<Vec<String>> {
    match (args.facet_filters.is_empty(), args.bs.is_empty()) {
        (true, _) => Ok(args.bs.clone()),
        (false, true) => facet_entity_ids(os, &args.facet_filters).await,
        (false, false) => {
            let facet_b_ids = facet_entity_ids(os, &args.facet_filters).await?;
            let set: HashSet<&str> = facet_b_ids.iter().map(String::as_str).collect();
            Ok(args
                .bs
                .iter()
                .filter(|b| set.contains(b.as_str()))
                .cloned()
                .collect())
        }
    }
}

/// Loads disease-target associations.
///
/// # Returns
/// A [`Paged`] with the [`Association`] entities.
/// # Errors
/// Returns an [`async_graphql::Error`] if the database query fails.
#[instrument(skip_all, level = "trace", fields(anchor = %anchor))]
pub async fn load_associations<A>(
    ctx: &Context<'_>,
    anchor: &str,
    args: &AssociationArguments,
) -> async_graphql::Result<Paged<Association<A::B>>>
where
    A: EntityWithAssociations,
    Association<A::B>: OutputType,
{
    if anchor.is_empty() {
        return Err("id is required".into());
    }

    let ch = ctx.data_unchecked::<ClickHouse>();
    let os = ctx.data_unchecked::<OpenSearch>();

    let a_ids = A::a_ids(ch, anchor).await?;
    tracing::trace!("found {} indirect ids", a_ids.len());
    let b_ids = prepare_b_ids(os, args).await?;
    if !args.facet_filters.is_empty() && b_ids.is_empty() {
        return Ok(Paged {
            count: 0,
            rows: vec![],
        });
    }

    let sql = AotfSql {
        table: A::TABLE,
        anchor: anchor.to_string(),
        a_ids,
        b_ids,
        b_filter: args.b_filter.clone(),
        policies: DatasourcePolicies::from_overrides(&args.datasource_policy_overrides),
        include_measurements: args.include_measurements,
        sort: args.sort.clone(),
        page: args.page,
    };

    let rows_sql = sql.build_query();
    tracing::trace!("{rows_sql:}");

    let rows = ch.query(&rows_sql).fetch_all::<AssociationRow>().await?;
    let count = if rows.is_empty() { 0 } else { rows[0].total };
    let rows = rows
        .into_iter()
        .map(AssociationRow::into_association::<A::B>)
        .collect();

    Ok(Paged { count, rows })
}

// ---- resolvers ----

#[ComplexObject]
impl Association<Disease> {
    /// Associated disease entity.
    async fn disease(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Disease>> {
        load_disease(ctx, &self.id).await
    }
}

#[ComplexObject]
impl Association<Target> {
    /// Associated target entity.
    async fn target(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Target>> {
        load_target(ctx, &self.id).await
    }
}
