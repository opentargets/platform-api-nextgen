use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, CustomValidator, InputObject, InputValueError, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{target::Target, variant::Chromosome},
    query::paginate::{Page, Paged},
};

// --- models ---

/// Region with chromosome, start and end positions.
#[derive(Debug, Clone, Deserialize, SimpleObject, InputObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Region {
    /// Chromosome
    chromosome: Chromosome,
    /// Start position
    start: u32,
    /// End position
    end: u32,
}

impl Region {
    #[must_use]
    pub fn new(chromosome: Chromosome, start: u32, end: u32) -> Self {
        Self {
            chromosome,
            start,
            end,
        }
    }
}

/// Region with chromosome, start and end positions.
#[derive(Debug, Clone, Deserialize, SimpleObject, InputObject)]
#[serde(rename_all = "camelCase")]
pub struct RegionInput {
    /// Chromosome
    chromosome: Chromosome,
    /// Start position
    start: u32,
    /// End position
    end: u32,
}

struct MaxRange {
    max: u32,
}

impl MaxRange {
    fn new(max: u32) -> Self { Self { max } }
}

impl CustomValidator<RegionInput> for MaxRange {
    fn check(&self, value: &RegionInput) -> Result<(), InputValueError<RegionInput>> {
        match value.end.checked_sub(value.start) {
            Some(diff) if diff <= self.max => Ok(()),
            Some(diff) => Err(InputValueError::custom(format!(
                "end - start must be less than {}, got {}",
                self.max, diff
            ))),
            None => Err(InputValueError::custom(
                "start ({}) must not be greater than end ({})",
            )),
        }
    }
}

#[derive(Debug, Clone, Deserialize, SimpleObject, Eq, PartialEq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    chromosome: Chromosome,
    start: u32,
    end: u32,
    index: u32,
    size: u32,
}

impl Key {
    #[must_use]
    pub fn new(chromosome: Chromosome, start: u32, end: u32, index: u32, size: u32) -> Self {
        Self {
            chromosome,
            start,
            end,
            index,
            size,
        }
    }
}

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TargetsByRegionRow {
    key: usize,
    targets: Paged<Target>,
}

// --- loaders ---

#[derive(From)]
pub struct TargetsByRegionLoader {
    ch: ClickHouse,
}

impl Loader<Key> for TargetsByRegionLoader {
    type Value = Paged<Target>;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[Key]) -> Result<HashMap<Key, Self::Value>, Self::Error> {
        let base_query = "
            WITH
                ? AS q_chromosome,
                ? AS q_start,
                ? AS q_end,
                ? AS q_offset,
                ? AS q_limit,
                CAST(? AS UInt64) AS query_id,
                filtered AS
                (
                    SELECT *
                    FROM targets_by_region
                    WHERE chromosome = q_chromosome
                      AND start <= q_end
                      AND end >= q_start
                ),
                (
                    SELECT count()
                    FROM filtered
                ) as count,
                paged AS
                (
                    SELECT *
                    FROM filtered
                    ORDER BY start, end
                    LIMIT q_limit OFFSET q_offset
                )
            SELECT
                query_id,
                tuple(
                        CAST(count AS UInt64),
                        groupArray(target) as rows
                    )
            FROM paged
            GROUP BY query_id
            ";
        let queries: Vec<String> = keys.iter().map(|_| base_query.to_string()).collect();
        let full_query = keys.iter().enumerate().fold(
            self.ch.query(&queries.join(" UNION ALL ")),
            |acc: clickhouse::query::Query, (i, key)| {
                acc.bind(key.chromosome)
                    .bind(key.start)
                    .bind(key.end)
                    .bind(key.index * key.size)
                    .bind(key.size)
                    .bind(i as u64)
            },
        );
        tracing::trace!("query is {}", full_query.sql_display());
        let result = full_query.fetch_all::<TargetsByRegionRow>().await?;
        let results_with_keys = result
            .iter()
            .map(|res| (keys[res.key].clone(), res.targets.clone()))
            .collect();

        Ok(results_with_keys)
    }
}

#[allow(clippy::missing_errors_doc)]
pub async fn load_targets_by_region(
    ctx: &async_graphql::Context<'_>,
    key: Key,
) -> async_graphql::Result<Paged<Target>> {
    Ok(ctx
        .data_unchecked::<DataLoader<TargetsByRegionLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}

#[ComplexObject]
impl Region {
    async fn targets(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for the Targets.")] page: Page,
    ) -> async_graphql::Result<Paged<Target>> {
        let key = Key::new(self.chromosome, self.start, self.end, page.index, page.size);
        load_targets_by_region(ctx, key).await
    }
}

// ---- resolvers ----
#[derive(Default)]
pub struct RegionQuery;

#[Object]
impl RegionQuery {
    #[allow(clippy::unused_async)]
    async fn region(
        &self,
        #[graphql(desc = "Region parameters.", validator(custom = "MaxRange::new(5000000)"))]
        region_input: RegionInput,
    ) -> async_graphql::Result<Region> {
        let region = Region::new(
            region_input.chromosome,
            region_input.start,
            region_input.end,
        );
        Ok(region)
    }
}
