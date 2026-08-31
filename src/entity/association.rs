use std::{collections::HashSet, f64::consts::PI, marker::PhantomData};

use async_graphql::{ComplexObject, Context, Enum, InputObject, OutputType, SimpleObject};
use clickhouse::Row;
use serde::Deserialize;
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

// ---- helpers ----

/// Precomputed harmonic series constant for scoring. See Basel Problem.
/// Note: this was `1.644_924_066_898_242_3` in the old Scala API, because the approximation method
/// was not precise enough. This makes the scores differ in their 5th decimal.
// const MAX_HS: f64 = 1.644_924_066_898_242_3;
const MAX_HS: f64 = PI * PI / 6.0;

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

/// Represents a datasource for association scoring.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Enum, EnumIter, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum Datasource {
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
    #[graphql(name = "GENE2PHENOTYPE")]
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

// *************************************************************************************************
// query arguemnts models

/// Represents the policy for a datasource.
#[derive(Debug, InputObject, Clone, Copy)]
struct DatasourcePolicy {
    /// The weight of the datasource in association scoring. Range is [0.0, 1.0].
    weight: f64,
    /// Whether the datasource should propagate its score to the overall association.
    propagate: bool,
    /// Whether the datasource is required for the association to be considered valid.
    required: bool,
}

impl Default for DatasourcePolicy {
    #[rustfmt::skip]
    fn default() -> Self { Self { weight: 1.0, propagate: true, required: false } }
}

impl Datasource {
    #[rustfmt::skip]
    fn default_policy(self) -> DatasourcePolicy {
        let d = DatasourcePolicy::default();
        match self {
            Self::CancerBiomarkers | Self::OtCrisprValidation | Self::OtCrispr | Self::Encore => { DatasourcePolicy { weight: 0.5, ..d } }
            Self::Europepmc | Self::Impc => DatasourcePolicy { weight: 0.2, ..d },
            Self::ExpressionAtlas => DatasourcePolicy { weight: 0.2, propagate: false, ..d },
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

/// Sort types. Contain the sort field and direction.
#[derive(Debug, InputObject)]
pub struct AssociationSort {
    /// The key to sort by. Can either be `score` to use the overall association score (default), a
    /// datasource id (e.g., `impc`), or a datatype id (e.g., `animal_model`).
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
    /// Expand the association set indirectly: for a disease, include its ontology descendants;
    /// for a target, include its interaction partners.
    pub indirect: bool,
    /// Whether to include diseases from the _measurement_ ontological family in B set. Default is
    /// `false`.
    pub include_measurements: Option<bool>,
    /// List of datasource policy overrides.
    pub datasource_policy_overrides: Vec<DatasourcePolicyOverride>,
    /// Ordering for the associations.
    pub sort: AssociationSort,
    /// Pagination for the associations.
    pub page: Page,
}

// *************************************************************************************************
// results models

/// A ClickHouse row representing an association, result from the AOTF query.
#[derive(Row, Deserialize)]
struct AssociationRow {
    #[serde(rename = "B")]
    id: String,
    score: f64,
    score_datatypes: Vec<(String, f64)>,
    score_datasources: Vec<(String, f64)>,
    novelty: Option<f64>,
    count: u64,
}

impl AssociationRow {
    #[instrument(skip_all, level = "trace", fields(id = %self.id, score = %self.score))]
    fn into_assoc<T: OutputType + 'static>(self) -> Association<T> {
        let map = |v: Vec<(String, f64)>| {
            v.into_iter()
                .map(|(id, score)| ScoredComponent { id, score })
                .collect()
        };
        Association {
            id: self.id,
            score: self.score,
            datatype_scores: map(self.score_datatypes),
            datasource_scores: map(self.score_datasources),
            novelty: self.novelty,
            _marker: PhantomData,
        }
    }
}

/// A scored component used in association scoring.
#[derive(Debug, Clone, SimpleObject)]
pub struct ScoredComponent {
    /// Component identifier (e.g., datatype or datasource name).
    id: String,
    /// Association score for the component. Scores are normalized to a range of 0-1. The higher
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
    /// Association scores computed for every datatype (e.g., Genetic associations, Somatic,
    /// Literature).
    datatype_scores: Vec<ScoredComponent>,
    /// Association scores computed for every datasource (e.g., IMPC, ChEMBL, Gene2Phenotype).
    datasource_scores: Vec<ScoredComponent>,
    /// A measure of how novel the target–disease association is, calculated based on the
    /// accumulation of direct evidence over time.
    novelty: Option<f64>,
    // marker for the actual embedded type (Disease or Target)
    #[graphql(skip)]
    _marker: PhantomData<T>,
}

pub type DiseaseAssociation = Association<Disease>;
pub type TargetAssociation = Association<Target>;

// *************************************************************************************************
// sql query related models

/// Contains the strings and other data needed to build an associations sql query.
pub struct AotfSql<'a> {
    table: &'static str,
    a_ids: Vec<String>,
    b_ids: Vec<String>,
    b_filter: Vec<String>,
    args: &'a AssociationArguments,
    weights: String,
    non_propagated: Vec<Datasource>,
    required: Vec<Datasource>,
}

/// A trait for entities that have associations.
pub trait EntityWithAssociations {
    /// The name of the association table.
    const TABLE: &'static str;
    /// The type of the B id for this entity's associations.
    type B: OutputType + 'static;
    /// Generate the ID set to use this entity as A in an association query.
    #[allow(async_fn_in_trait)]
    async fn a_ids(
        ch: &ClickHouse,
        anchor: &str,
        indirect: bool,
    ) -> async_graphql::Result<Vec<String>>;
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

/// Loads disease-target associations
/// # Returns
/// A [`Paged`] with the [`Assocation`] entities.
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

    let a_ids = A::a_ids(ch, anchor, args.indirect).await?;
    tracing::trace!("propagated to {} ids", a_ids.len());
    let b_ids = prepare_b_ids(os, args).await?;
    if !args.facet_filters.is_empty() && b_ids.is_empty() {
        return Ok(Paged {
            total: 0,
            items: vec![],
        });
    }

    let sql = AotfSql::new(A::TABLE, a_ids, b_ids, args);
    let rows_sql = sql.build_query();
    tracing::trace!("{rows_sql:}");

    let rows = ch.query(&rows_sql).fetch_all::<AssociationRow>().await?;
    let total = if rows.is_empty() { 0 } else { rows[0].count };
    let items = rows
        .into_iter()
        .map(AssociationRow::into_assoc::<A::B>)
        .collect();

    Ok(Paged { total, items })
}

// ---- query builder ----

impl<'a> AotfSql<'a> {
    /// Returns the anchor ID of the association.
    fn anchor(&self) -> &str { &self.a_ids[0] }

    /// Constructs a new `AotfSql` instance with the given parameters.
    fn new(
        table: &'static str,
        a_ids: Vec<String>,
        b_ids: Vec<String>,
        args: &'a AssociationArguments,
    ) -> Self {
        let policies: Vec<(Datasource, DatasourcePolicy)> = Datasource::iter()
            .map(|ds| {
                let p = args
                    .datasource_policy_overrides
                    .iter()
                    .find(|o| o.id == ds)
                    .map_or_else(|| ds.default_policy(), |o| o.policy);
                (ds, p)
            })
            .collect();

        let b_filter = args
            .b_filter
            .as_deref()
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_lowercase)
            .map(|t| esc(&t))
            .collect();

        let weights = policies
            .iter()
            .map(|(ds, p)| format!("('{}', {:?})", esc((*ds).into()), p.weight))
            .collect::<Vec<_>>()
            .join(", ");

        let non_propagated = policies
            .iter()
            .filter(|(_, p)| !p.propagate)
            .map(|(ds, _)| *ds)
            .collect();

        let required = policies
            .iter()
            .filter(|(_, p)| p.required)
            .map(|(ds, _)| *ds)
            .collect();

        Self {
            table,
            a_ids,
            b_ids,
            b_filter,
            args,
            weights,
            non_propagated,
            required,
        }
    }

    /// Returns the WHERE clause for the association query.
    ///
    /// This is what compiles the set of association rows that will make it to the score calculation
    /// for each `Datasource` in the query.
    fn prewhere(&self) -> String {
        let mut conj = Vec::new();

        // `anchor` term: keep the row if its `A` is in our `a_ids` set.
        conj.push(if self.non_propagated.is_empty() {
            format!("A IN ({})", quoted_set(&self.a_ids))
        // `non_propagated` sources present: keep the row if `A` is the anchor, or `A` is in
        // `a_ids` and the `datasourceId` is not in the `non_propagated` set of `Datasources`.
        } else {
            format!(
                "((A IN ({}) AND datasourceId NOT IN ({})) OR A = '{}')",
                quoted_set(&self.a_ids),
                quoted_set(&self.non_propagated),
                self.anchor(),
            )
        });

        // `b_filter`: separate `LIKE` clauses. Keep the row if _all match_.
        conj.extend(
            self.b_filter
                .iter()
                .map(|f| format!("searchB LIKE lower('%{f}%')")),
        );

        // `b_ids`: keep the row if its `B` is in our `b_ids` set.
        if !self.b_ids.is_empty() {
            conj.push(format!("B IN ({})", quoted_set(&self.b_ids)));
        }

        // `include_measurements`: keep the row unless it is a measurement and the policy is
        // explicitly set to `false`.
        if self.args.include_measurements == Some(false) {
            conj.push("isMeasurement = false".into());
        }

        // `required`: keep the row if its `B` appears in some row whose `A` is in `a_ids` and whose
        // `datasourceId` is in the `required` set. Gates B, doesn't filter scoring rows.
        if !self.required.is_empty() {
            let anchor = if self.non_propagated.is_empty() {
                format!("A IN ({})", quoted_set(&self.a_ids))
            } else {
                format!(
                    "((A IN ({}) AND datasourceId NOT IN ({})) OR A = '{}')",
                    quoted_set(&self.a_ids),
                    quoted_set(&self.non_propagated),
                    self.anchor(),
                )
            };
            conj.push(format!(
                "B IN (SELECT B FROM {} PREWHERE {anchor} AND datasourceId IN ({}))",
                self.table,
                quoted_set(&self.required),
            ));
        }

        // Join it all with AND, so all conditions must be met for the row to be included.
        conj.join(" AND ")
    }

    #[must_use]
    fn build_query(&self) -> String {
        format!("
WITH
    {max_hs} AS max_hs_score,
    arrayReverseSort(x -> x.2, groupArray((score_datasource / max_hs_score, (score_datasource * datasource_weight) / max_hs_score, datasourceId, datatypeId))) AS scores_vector,
    arrayMap((i, j) -> (i.1, i.2 / pow(j, 2), i.3, i.4), scores_vector, arrayEnumerate(scores_vector)) AS datasource_scores,
    arrayMap(x -> (x.3, x.1), datasource_scores) AS score_datasources,
    arrayMap(x -> (x.4, x.1), datasource_scores) AS score_dt,
    groupUniqArray(datatypeId) AS datatypes_v,
    arrayMap(x -> (x, arrayReverseSort(arrayMap(b -> b.2, arrayFilter(a -> a.1 = x, score_dt)))), datatypes_v) AS mapped_dts,
    arrayMap(x -> (x.1, arraySum((i, j) -> i / pow(j, 2), x.2, arrayEnumerate(x.2)) / arraySum(arrayMap((x, y) -> x / pow(y, 2), replicate(1.0, x.2), arrayEnumerate(x.2)))), mapped_dts) AS score_datatypes,
    arraySum(datasource_scores.2) / max_hs_score AS score,
    any(noveltyWhereA) AS novelty,
    concat(score_datatypes, score_datasources) AS joint_scores,
    if(indexOf(joint_scores.1, '{order_name}') != 0, joint_scores[indexOf(joint_scores.1, '{order_name}')].2, 0.0) AS score_indexed
SELECT
    B, score, score_datatypes, score_datasources, novelty,
    count() OVER () AS total
FROM (
    WITH
        arraySum(arrayMap((x, y) -> x / pow(y, 2), arrayReverseSort(groupArray(rowScore)), arrayEnumerate(groupArray(rowScore)))) AS score_datasource,
        any(datatypeId) AS datatypeId,
        ifNull(any(weight), 1.0) AS datasource_weight
    SELECT
        B, datasource_weight, datatypeId, datasourceId, score_datasource,
        anyIf({novelty}, A = '{a_id}') AS noveltyWhereA
    FROM {table} AS l
    LEFT JOIN (
        WITH arrayJoin([{weights}]) AS weightPair
        SELECT weightPair.1 AS datasourceId, toNullable(weightPair.2) AS weight
        ORDER BY datasourceId ASC
    ) AS r USING (datasourceId)
    PREWHERE {prewhere}
    GROUP BY B, datasourceId
)
GROUP BY B
ORDER BY {order_by} {dir}
LIMIT {offset}, {size}
            ",
            max_hs = MAX_HS,
            order_name = esc(&self.args.sort.key),
            novelty = if self.args.indirect {
                "noveltyIndirect"
            } else {
                "noveltyDirect"
            },
            a_id = self.anchor(),
            table = self.table,
            weights = self.weights,
            prewhere = self.prewhere(),
            dir = self.args.sort.direction.as_sql(),
            order_by = match self.args.sort.key.as_str() {
                "score" => "score",
                "novelty" => "novelty",
                _ => "score_indexed",
            },
            offset = self.args.page.index * self.args.page.size,
            size = self.args.page.size,
        )
    }
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
