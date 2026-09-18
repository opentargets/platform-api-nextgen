use std::{collections::HashMap, sync::LazyLock};

use async_graphql::{
    Context, Enum, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use moka::future::Cache;
use serde::{Deserialize, Deserializer, de::IntoDeserializer};

use crate::{
    datasource::clickhouse::{ClickHouse, from_string},
    entity::association::Datasource,
    query::{
        QueryExt,
        cache::{CachedLoader, entity_cache},
        paginate::{Page, Paged},
    },
};

// ---- models ----

///  Type of aggregation used to calculate the yearly evidence count.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Enum, Deserialize)]
#[serde(rename_all = "camelCase")]
#[graphql(rename_items = "camelCase")]
pub enum AggregationType {
    /// Aggregates all the evidence of all datasources into an overall score.
    Overall,
    /// Aggregates the evidence of each datasource separately.
    DatasourceId,
}

/// Deserialize a `Datasource` from a `String` column, allowing `None` values.
///
/// We should fix the model in ClickHouse so this becomes a nullable enum in there.
fn datasource_or_none<'de, D>(d: D) -> Result<Option<Datasource>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(d)?;
    if s == "None" {
        return Ok(None);
    }
    Datasource::deserialize(s.into_deserializer()).map(Some)
}

/// Association time series entry for a target-disease association.
#[derive(Debug, Clone, Deserialize, SimpleObject, Row)]
#[serde(rename_all = "camelCase")]
pub struct AssociationTimeseries {
    /// EFO ID of the association disease.
    disease_id: String,
    /// Ensembl ID of the association target.
    target_id: String,
    /// Type of aggregation used to calculate the yearly evidence count.
    #[serde(deserialize_with = "from_string")]
    aggregation_type: AggregationType,
    /// Datasource identifier of the aggregation: eg. `gwas_credible_set` or `None` if the
    /// aggregation is by overall score.
    #[serde(deserialize_with = "datasource_or_none")]
    aggregation_value: Option<Datasource>,
    /// Flag indicating whether the novelty calculation is based on direct evidence only or
    /// includes indirect evidence.
    is_direct: bool,
    /// Year up to and including which evidence is considered.
    year: Option<u16>,
    /// Association score based on evidence up until a given year.
    association_score: f64,
    /// Novelty of the association at the given year.
    novelty: Option<f64>,
    /// Yearly count of evidence supporting the disease/target association (for a given
    /// datasource).
    yearly_evidence_count: Option<u32>,
}

// ---- loaders ----

/// The key for filtering timeseries.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct TimeseriesKey {
    pub disease_id: String,
    pub target_id: String,
}

pub type TimeseriesCache = Cache<TimeseriesKey, Option<Vec<AssociationTimeseries>>>;
static TIMESERIES_CACHE: LazyLock<TimeseriesCache> = LazyLock::new(entity_cache);

impl CachedLoader for AssociationTimeseriesLoader {
    type Key = TimeseriesKey;
    type Value = Vec<AssociationTimeseries>;

    fn cache(&self) -> &TimeseriesCache { &TIMESERIES_CACHE }

    fn key_of(v: &Self::Value) -> Self::Key {
        TimeseriesKey {
            disease_id: v[0].disease_id.clone(),
            target_id: v[0].target_id.clone(),
        }
    }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        let pairs: Vec<(&str, &str)> = misses
            .iter()
            .map(|k| (k.disease_id.as_str(), k.target_id.as_str()))
            .collect();
        let rows = self
            .ch
            .query("SELECT ?fields FROM association_time_series WHERE (diseaseId, targetId) IN ?")
            .bind(pairs)
            .fetch_all::<AssociationTimeseries>()
            .await?;

        // Split rows into groups by (diseaseId, targetId) to put in the cache.
        let mut groups: HashMap<TimeseriesKey, Self::Value> = HashMap::new();
        for r in rows {
            let key = TimeseriesKey {
                disease_id: r.disease_id.clone(),
                target_id: r.target_id.clone(),
            };
            groups.entry(key).or_default().push(r);
        }
        Ok(groups.into_values().collect())
    }
}

impl Loader<TimeseriesKey> for AssociationTimeseriesLoader {
    type Value = Vec<AssociationTimeseries>;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[TimeseriesKey],
    ) -> Result<HashMap<TimeseriesKey, Self::Value>, Self::Error> {
        self.load_cached(keys).await
    }
}

#[derive(From)]
pub struct AssociationTimeseriesLoader {
    ch: ClickHouse,
}

// ---- resolvers ----

pub struct AssociationTimeseriesArguments {
    pub is_direct: bool,
    pub aggregation_types: Option<Vec<AggregationType>>,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub page: Page,
}

/// Load the time series for a disease or target anchor, filtered and paged.
///
/// # Errors
/// Returns an [`async_graphql::Error`] if the database query fails.
pub async fn load_association_timeseries(
    ctx: &Context<'_>,
    key: TimeseriesKey,
    args: &AssociationTimeseriesArguments,
) -> async_graphql::Result<Paged<AssociationTimeseries>> {
    if let (Some(s), Some(e)) = (args.start_year, args.end_year)
        && s > e
    {
        return Err(async_graphql::Error::new(
            "start_year must be less than or equal to end_year",
        ));
    }

    let rows = ctx
        .data_unchecked::<DataLoader<AssociationTimeseriesLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default();

    let in_year_range = |y: Option<u16>| match y {
        Some(y) => {
            let y = i32::from(y);
            args.start_year.is_none_or(|s| y >= s) && args.end_year.is_none_or(|e| y <= e)
        }
        None => args.start_year.is_none() && args.end_year.is_none(),
    };

    // Filtering on arguments is done outside of the db as disease + target returns a relatively
    // small number of rows.
    let items: Vec<_> = rows
        .into_iter()
        .filter(|r| r.is_direct == args.is_direct)
        .filter(|r| {
            args.aggregation_types
                .as_ref()
                .is_none_or(|ts| ts.contains(&r.aggregation_type))
        })
        .filter(|r| in_year_range(r.year))
        .collect();

    Ok(items.query().paginate(args.page))
}
