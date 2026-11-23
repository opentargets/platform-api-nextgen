use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, InputObject, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        target::{Target, load_targets},
        variant::Chromosome,
    },
    query::paginate::{Page, Paged, page_complexity},
};

// --- models ---

/// Region with chromosome, start and end positions.
#[derive(Debug, Clone, Deserialize, SimpleObject, InputObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Region {
    /// Chromosome the region is contained in.
    chromosome: Chromosome,
    /// Start position.
    start: u32,
    /// End position.
    end: u32,
}

const MAX_REGION_SPAN: u32 = 5_000_000;

impl Region {
    #[allow(clippy::missing_errors_doc)]
    pub fn try_new(chromosome: Chromosome, start: u32, end: u32) -> async_graphql::Result<Self> {
        match end.checked_sub(start) {
            Some(span) if span <= MAX_REGION_SPAN => Ok(Self {
                chromosome,
                start,
                end,
            }),
            Some(span) => Err(async_graphql::Error::new(format!(
                "end - start must not exceed {MAX_REGION_SPAN}, got {span}"
            ))),
            None => Err(async_graphql::Error::new(format!(
                "start ({start}) must not be greater than end ({end})"
            ))),
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

/// One page of the target ids in a region, and the number of targets in the region.
#[derive(Debug, Clone, Default)]
pub struct TargetIdsPage {
    count: u64,
    ids: Vec<String>,
}

#[derive(Debug, Clone, Row, Deserialize)]
struct TargetIdsByRegionRow {
    key: u64,
    count: u64,
    ids: Vec<String>,
}

// --- loaders ---

#[derive(From)]
pub struct TargetsByRegionLoader {
    ch: ClickHouse,
}

impl Loader<Key> for TargetsByRegionLoader {
    type Value = TargetIdsPage;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[Key]) -> Result<HashMap<Key, Self::Value>, Self::Error> {
        let base_query = "
            WITH
                ? AS q_chromosome,
                ? AS q_start,
                ? AS q_end,
                ? AS q_offset,
                ? AS q_limit,
                filtered AS
                (
                    SELECT start, end, tupleElement(target, 'id') AS id
                    FROM targets_by_region
                    WHERE chromosome = q_chromosome
                      AND start <= q_end
                      AND end >= q_start
                ),
                paged AS
                (
                    SELECT id
                    FROM filtered
                    ORDER BY start, end, id
                    LIMIT q_limit OFFSET q_offset
                )
            SELECT
                CAST(? AS UInt64) AS key,
                ifNull((SELECT count() FROM filtered), 0) AS count,
                (SELECT groupArray(id) FROM paged) AS ids
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
        let rows = full_query.fetch_all::<TargetIdsByRegionRow>().await?;
        rows.into_iter()
            .map(|row| {
                let key = usize::try_from(row.key)
                    .ok()
                    .and_then(|i| keys.get(i))
                    .ok_or_else(|| format!("unknown region query id {}", row.key))?;
                Ok((
                    key.clone(),
                    TargetIdsPage {
                        count: row.count,
                        ids: row.ids,
                    },
                ))
            })
            .collect()
    }
}

/// Loads one page of the targets in a region: their ids from `targets_by_region`, then the
/// targets themselves through [`load_targets`].
#[allow(clippy::missing_errors_doc)]
pub async fn load_targets_by_region(
    ctx: &async_graphql::Context<'_>,
    key: Key,
) -> async_graphql::Result<Paged<Target>> {
    let page = ctx
        .data_unchecked::<DataLoader<TargetsByRegionLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default();
    Ok(Paged {
        count: page.count,
        rows: load_targets(ctx, &page.ids).await?,
    })
}

#[ComplexObject]
impl Region {
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn targets(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for the targets.")] page: Page,
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
        #[graphql(desc = "Chromosome the region is contained in.")] chromosome: Chromosome,
        #[graphql(desc = "Start position.")] start: u32,
        #[graphql(desc = "End position.")] end: u32,
    ) -> async_graphql::Result<Region> {
        Region::try_new(chromosome, start, end)
    }
}
