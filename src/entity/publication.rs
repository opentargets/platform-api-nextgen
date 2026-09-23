use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        paginate::{Page, PagedWithStats},
        stats::HasStats,
    },
};

// ---- models ----

/// Referenced publication information.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Publication {
    /// PubMed identifier [bioregistry:pubmed].
    pmid: String,
    /// PubMed Central identifier (if available) [bioregistry:pmc].
    pmcid: Option<String>,
    #[graphql(skip)]
    year: u16,
    #[graphql(skip)]
    month: u8,
}

#[ComplexObject]
impl Publication {
    /// Publication date.
    async fn publication_date(&self) -> String { format!("{:04}-{:02}-01", self.year, self.month) }
}

/// Statistics for a set of studies.
#[derive(Clone, SimpleObject)]
pub struct PublicationStats {
    /// Earliest publication date for the target without considering the time range.
    earliest_pub_date: u16,
}

impl HasStats for Publication {
    type Stats = PublicationStats;
}

pub struct PublicationArguments {
    pub ids: Vec<String>,
    pub start_year: Option<u32>,
    pub start_month: Option<u32>,
    pub end_year: Option<u32>,
    pub end_month: Option<u32>,
    pub page: Page,
}

/// A key used to uniquely identify a set of literature results.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LiteratureKey {
    pub ids: Vec<String>,
    pub start: (u32, u32), // (year, month)
    pub end: (u32, u32),
    pub page: Page,
}

/// A ClickHouse row representing a publication along some calculated stats.
#[derive(Row, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiteratureRow {
    key: u64,
    count: u64,
    earliest_pub_year: u16,
    rows: Vec<Publication>,
}

// ---- loaders ----

#[derive(From)]
pub struct PublicationLoader {
    ch: ClickHouse,
}

const QUERY: &str = "
WITH
    (year, month) BETWEEN (?, ?) AND (?, ?) AS in_range
SELECT
    CAST(? AS UInt64) AS key,
    countIf(in_range) AS count,
    min(year) AS earliestPubYear,
    arraySlice(                                                 -- 3.  Slice the array using 3a
        arrayReverseSort(                                       -- 2.  Sort reverse using 1c
            (row, sort_key) -> sort_key,                        -- 1c. Lambda for sorting (returns 1a)
            groupArrayIf((pmid, pmcid, year, month), in_range), -- 1b. Return tuple
            groupArrayIf((relevance, year, month), in_range)    -- 1a. Sort tuple: relevance, year, month
        ),
        ?, ?                                                    -- 3a. offset (index*size+1), size
    ) AS rows
FROM literature_entity_lut
WHERE keywordId IN ?                                            -- Filter by passed ids
";

impl Loader<LiteratureKey> for PublicationLoader {
    type Value = PagedWithStats<Publication>;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[LiteratureKey],
    ) -> Result<HashMap<LiteratureKey, Self::Value>, Self::Error> {
        // Make a vector of "{QUERY} UNION ALL {QUERY}" binding all `?` in every iteration.
        // Use the enumeration index as key to match back results.
        let sql = vec![QUERY; keys.len()].join(" UNION ALL ");
        let query = keys
            .iter()
            .enumerate()
            .fold(self.ch.query(&sql), |q, (i, k)| {
                q.bind(k.start.0)
                    .bind(k.start.1)
                    .bind(k.end.0)
                    .bind(k.end.1)
                    .bind(i as u64)
                    .bind(k.page.index * k.page.size + 1)
                    .bind(k.page.size)
                    .bind(&k.ids)
            });

        // Run query.
        let rows: Vec<LiteratureRow> = query.fetch_all().await?;

        // Build `PagedWithStats` for each result row and put it in a `HashMap` with the keys coming
        // from the original `LiteratureKey`.
        Ok(rows
            .into_iter()
            .map(|r| {
                let value = PagedWithStats {
                    count: r.count,
                    rows: r.rows,
                    stats: PublicationStats {
                        earliest_pub_date: r.earliest_pub_year,
                    },
                };
                #[allow(clippy::cast_possible_truncation)]
                (keys[r.key as usize].clone(), value)
            })
            .collect())
    }
}

/// Loads Publications using a `PublicationArguments` struct.
///
/// # Returns
/// A `Vec` of `Publication` objects corresponding to the given `PublicationArguments`.
/// # Errors
/// Returns an error if the Publications could not be loaded.
pub async fn load_publications(
    ctx: &Context<'_>,
    args: PublicationArguments,
) -> async_graphql::Result<Option<PagedWithStats<Publication>>> {
    let key = LiteratureKey {
        ids: args.ids,
        start: (args.start_year.unwrap_or(0), args.start_month.unwrap_or(1)),
        end: (args.end_year.unwrap_or(9999), args.end_month.unwrap_or(12)),
        page: args.page,
    };
    ctx.data_unchecked::<DataLoader<PublicationLoader>>()
        .load_one(key)
        .await
}
