// ---- models ----

use std::{any::TypeId, collections::HashMap, marker::PhantomData, sync::LazyLock};

use async_graphql::{ComplexObject, Context, InputObject, OutputType, SimpleObject};
use clickhouse::Row;
use serde::Deserialize;

use crate::{
    entity::{
        disease::{Disease, load_disease},
        target::{Target, load_target},
    },
    query::{
        paginate::{Page, Paged},
        sort::SortDirection,
    },
};

/// Defines the policy for a datasource, including its weight, propagation, and required status.
#[derive(Clone, Copy)]
pub struct DatasourcePolicy {
    /// The weight of the datasource in association scoring. Range is [0.0, 1.0].
    weight: f64,
    /// Whether the datasource should propagate its score to the overall association.
    propagate: bool,
    /// Whether the datasource is required for the association to be considered valid.
    required: bool,
}

impl Default for DatasourcePolicy {
    /// Returns a default policy with weight 1.0, propagate true, and required false.
    #[rustfmt::skip]
    fn default() -> Self { Self { weight: 1.0, propagate: true, required: false } }
}

// overrides only; everything else takes Default
#[rustfmt::skip]
static DEFAULT_POLICY: LazyLock<HashMap<&str, DatasourcePolicy>> = LazyLock::new(|| {
    HashMap::from([
        ("europepmc",            DatasourcePolicy { weight: 0.2, propagate: true,  required: false }),
        ("expression_atlas",     DatasourcePolicy { weight: 0.2, propagate: false, required: false }),
        ("impc",                 DatasourcePolicy { weight: 0.2, propagate: true,  required: false }),
        ("cancer_biomarkers",    DatasourcePolicy { weight: 0.5, propagate: true,  required: false }),
        ("ot_crispr_validation", DatasourcePolicy { weight: 0.5, propagate: true,  required: false }),
        ("ot_crispr",            DatasourcePolicy { weight: 0.5, propagate: true,  required: false }),
        ("encore",               DatasourcePolicy { weight: 0.5, propagate: true,  required: false }),
    ])
});

/// Server default policy for `id` (override if listed, else `Default`).
pub fn default_policy(id: &str) -> DatasourcePolicy {
    DEFAULT_POLICY.get(id).copied().unwrap_or_default()
}

/// Input type for `DatasourcePolicy`.
#[derive(InputObject, Clone)]
pub struct DatasourcePolicyInput {
    /// The ID of the datasource.
    id: String,
    /// The weight of the datasource in association scoring. Range is [0.0, 1.0].
    weight: f64,
    /// Whether the datasource should propagate its score to the overall association.
    propagate: bool,
    /// Whether the datasource is required for the association to be considered valid.
    #[graphql(default)]
    required: bool,
}

/// Client overrides only (id → policy). Absent ids fall back to `default_policy`.
fn resolve_policies(input: Option<&[DatasourcePolicyInput]>) -> HashMap<String, DatasourcePolicy> {
    input
        .into_iter()
        .flatten()
        .map(|s| {
            (
                s.id.clone(),
                DatasourcePolicy {
                    weight: s.weight,
                    propagate: s.propagate,
                    required: s.required,
                },
            )
        })
        .collect()
}

/// Effective policy for `id`: client override wins, else server default.
fn policy_for(overrides: &HashMap<String, DatasourcePolicy>, id: &str) -> DatasourcePolicy {
    overrides
        .get(id)
        .copied()
        .unwrap_or_else(|| default_policy(id))
}

/// Sort types. Contain the sort field and direction.
#[derive(InputObject, Clone)]
pub struct AssociationSort {
    /// The key to sort by. Can be either "score" to sort by overall score, a datasource or a
    /// datatype.
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
pub struct AssocArgs {
    /// List of disease or target ids to use as the second dimension for associations.
    pub bs: Vec<String>,
    /// Filter to apply to the B dimension items.
    pub b_filter: Option<String>,
    /// List of the facet IDs to filter by (using AND).
    pub facet_filters: Vec<String>,
    /// Expand the association set indirectly: for a disease, include its ontology descendants;
    /// for a target, include its interaction partners.
    pub indirect: bool,
    /// Whether to include measurements in the response.
    pub include_measurements: bool,
    /// List of datasource policies. If ommitted, use the default.
    pub datasources: Option<Vec<DatasourcePolicyInput>>,
    /// Ordering for the associations. Can either be `score` to use the overall association score
    /// (default), a datasource id (e.g., `impc`), or a datatype id (e.g., `animal_model`)."
    pub sort: AssociationSort,
    /// Pagination for the associations.
    pub page: Page,
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
    /// stronger association between the target and the disease. Scores are normalized to a range
    /// of 0-1.
    score: f64,
    /// Association scores computed for every datatype (e.g., Genetic associations, Somatic,
    /// Literature).
    datatype_scores: Vec<ScoredComponent>,
    /// Association scores computed for every datasource (e.g., IMPC, ChEMBL, Gene2Phenotype).
    datasource_scores: Vec<ScoredComponent>,
    /// A measure of how novel the target–disease association is, calculated based on the
    /// accumulation of direct evidence over time.
    novelty: Option<f64>,
    #[graphql(skip)]
    _marker: PhantomData<T>,
}

// A ClickHouse row representing an association, result from the AOTF query.
#[derive(Row, Deserialize)]
pub struct AssociationRow {
    #[serde(rename = "B")]
    id: String,
    score: f64,
    score_datatypes: Vec<(String, f64)>,
    score_datasources: Vec<(String, f64)>,
    novelty: Option<f64>,
}

impl AssociationRow {
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

pub type DiseaseAssociation = Association<Disease>;
pub type TargetAssociation = Association<Target>;

// ---- loaders ----

fn fake<T: OutputType + 'static>(i: usize) -> Association<T> {
    let id: String = if TypeId::of::<T>() == TypeId::of::<Disease>() {
        "MONDO_0004992".into()
    } else {
        "ENSG00000157764".into()
    };

    Association {
        id,
        score: 0.9 - i as f64 * 0.05,
        datatype_scores: vec![ScoredComponent {
            id: "genetic_association".into(),
            score: 0.8,
        }],
        datasource_scores: vec![ScoredComponent {
            id: "eva".into(),
            score: 0.8,
        }],
        novelty: Some(0.1),
        _marker: PhantomData,
    }
}

#[allow(clippy::unnecessary_wraps)]
fn mock_associations<T>(
    _ctx: &Context<'_>,
    _fixed_id: &str,
) -> async_graphql::Result<Paged<Association<T>>>
where
    T: OutputType + 'static,
    Association<T>: OutputType,
{
    let items = (0..10).map(fake::<T>).collect();
    Ok(Paged { total: 10, items })
}

async fn fetch_associations<T: OutputType + 'static>(
    ch: &clickhouse::Client,
    sql: String,
) -> async_graphql::Result<Vec<Association<T>>> {
    let rows = ch.query(&sql).fetch_all::<AssociationRow>().await?;
    Ok(rows
        .into_iter()
        .map(AssociationRow::into_assoc::<T>)
        .collect())
}

/// Loads disease-target associations
/// # Returns
/// A vector of [`Association`] entities.
/// # Errors
/// Returns an [`async_graphql::Error`] if the database query fails.
pub fn load_associations<T>(
    ctx: &Context<'_>,
    fixed_id: &str,
    args: AssocArgs,
) -> async_graphql::Result<Paged<Association<T>>>
where
    T: OutputType + 'static,
    Association<T>: OutputType,
{
    mock_associations(ctx, fixed_id)
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
