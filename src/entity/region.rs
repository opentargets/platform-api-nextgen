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
    query::{
        paginate::{Page, Paged, page_complexity},
        sort::SortDirection,
        sql::{Column, Filter, Op, OrderBy, PageSpec, Table, Value, ValueKind, build, columns},
    },
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

#[derive(Debug, Clone, Row, Deserialize)]
struct RegionTarget {
    id: String,
    #[allow(dead_code, reason = "read only to keep the column order")]
    chromosome: Chromosome,
    #[allow(dead_code, reason = "read only to keep the column order")]
    start: u32,
    #[allow(dead_code, reason = "read only to keep the column order")]
    end: u32,
}

impl Table for RegionTarget {
    type Col = RegionTargetCol;
    const NAME: &'static str = "(
        SELECT
            target.id AS id,
            chromosome,
            start,
            end
        FROM targets_by_region
    )";
    const TIE_BREAKER: &'static str = "id";
}

columns!(RegionTargetCol {
    Chromosome => "chromosome" : I64,
    Start      => "start"      : U32,
    End        => "end"        : U32,
});

impl Key {
    /// Targets overlapping the region, in genomic order.
    fn to_spec(&self) -> async_graphql::Result<PageSpec<RegionTarget>> {
        use RegionTargetCol as C;
        let filters = vec![
            Filter::new(C::Chromosome, Op::Eq, Value::I64(self.chromosome as i64)),
            Filter::new(C::Start, Op::Le, Value::U32(self.end)),
            Filter::new(C::End, Op::Ge, Value::U32(self.start)),
        ]
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(async_graphql::Error::new)?;
        Ok(PageSpec {
            filters,
            order_by: OrderBy::each(&[C::Start, C::End], SortDirection::Ascending),
            offset: u64::from(self.index) * u64::from(self.size),
            limit: u64::from(self.size),
        })
    }
}

/// Total target count in a region and the target ids on one page, in genomic order.
#[derive(Debug, Clone, Default)]
pub struct RegionTargetIds {
    count: u64,
    ids: Vec<String>,
}

// --- loaders ---

/// One row of the batched query: the total and the page of targets for `keys[key]`.
#[derive(Debug, Clone, Row, Deserialize)]
struct BatchRow {
    key: usize,
    page: (u64, Vec<RegionTarget>),
}

#[derive(From)]
pub struct TargetsByRegionLoader {
    ch: ClickHouse,
}

impl Loader<Key> for TargetsByRegionLoader {
    type Value = RegionTargetIds;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[Key]) -> Result<HashMap<Key, Self::Value>, Self::Error> {
        let rendered = keys
            .iter()
            .map(|k| k.to_spec().map(|spec| spec.render()))
            .collect::<async_graphql::Result<Vec<_>>>()?;

        let rows = build(self.ch.client(), &rendered)
            .fetch_all::<BatchRow>()
            .await?;

        Ok(rows
            .into_iter()
            .map(
                |BatchRow {
                     key,
                     page: (count, rows),
                 }| {
                    let ids = rows.into_iter().map(|r| r.id).collect();
                    (keys[key].clone(), RegionTargetIds { count, ids })
                },
            )
            .collect())
    }
}

#[allow(clippy::missing_errors_doc)]
pub async fn load_targets_by_region(
    ctx: &async_graphql::Context<'_>,
    key: Key,
) -> async_graphql::Result<Paged<Target>> {
    let RegionTargetIds { count, ids } = ctx
        .data_unchecked::<DataLoader<TargetsByRegionLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default();
    let rows = load_targets(ctx, &ids).await?;
    Ok(Paged { count, rows })
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
