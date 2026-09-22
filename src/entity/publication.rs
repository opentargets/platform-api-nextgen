use std::collections::HashMap;

use async_graphql::{
    Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::paginate::{Page, Paged},
};

// ---- models ----

/// Referenced publication information.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Publication {
    /// PubMed identifier [bioregistry:pubmed].
    pmid: String,
    /// PubMed Central identifier (if available) [bioregistry:pmc].
    pmcid: Option<String>,
    /// Publication date.
    #[graphql(name = "publicationDate")]
    date: String,
    /// Year of publication.
    #[graphql(skip)]
    #[allow(unused)]
    year: u16,
    /// Month of publication.
    #[graphql(skip)]
    #[allow(unused)]
    month: u8,
    #[graphql(skip)]
    #[allow(unused)]
    /// Relevance score of the keyword within the literature entry.
    relevance: f64,
}

/// List of referenced publications with total counts, filtered counts, earliest year.
#[derive(Debug, Clone, Row, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Publications {
    /// Total number of publications matching the query.
    pub count: u64,
    /// Earliest publication year.
    pub earliest_pub_year: u32,
    /// Number of publications after applying filters.
    pub filtered_count: u64,
    key: u64,
    /// List of publications.
    pub rows: Vec<Publication>,
}

/// Paginated list of referenced publications with total counts and earliest year.
#[derive(Debug, Clone, Row, SimpleObject, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LiteratureOcurrences {
    /// Earliest publication date for a target.
    pub earliest_pub_year: u32,
    /// Total number of publications matching the query.
    pub total_count: u64,
    /// List of publications.
    pub publications: Paged<Publication>,
}

/// Statistics for a set of studies.
#[derive(SimpleObject)]
pub struct PublicationStats {
    /// Earliest publication date for a target.
    earliest_pub_date: u32,
    /// The distribution of studies with sumstats.
    filtered_count: u64,
}

// ---- loaders ----

#[derive(From)]
pub struct PublicationLoader {
    ch: ClickHouse,
}

#[allow(clippy::type_complexity)]
fn build_query(key: (&u64, &(Vec<String>, Option<u32>, Option<u32>, Page))) -> String {
    let start_date_filter = match &key.1.1 {
        Some(_) => {
            "greaterOrEquals(plus(multiply(if(equals(year, 0), 1970, year), 100), month),q_start_date)"
        }
        None => "true",
    };

    let end_date_filter = match &key.1.2 {
        Some(_) => {
            "lessOrEquals(plus(multiply(if(equals(year, 0), 1970, year), 100), month),q_end_date)"
        }
        None => "true",
    };

    let date_filter = format!("and({start_date_filter},{end_date_filter})");

    let filter = format!("and(in(keywordId,q_target_id), {date_filter})");

    let query = format!("WITH ? AS q_start_date,
        ? AS q_end_date,
        ? AS q_target_id,
        (SELECT min(if(equals(year, 0), 1970, year)) FROM platform2609.literature_entity_lut WHERE in(keywordId,q_target_id)) as ly,
        (SELECT count(pmid) FROM platform2609.literature_entity_lut WHERE in(keywordId,q_target_id)) as c,
        (SELECT count(pmid) FROM platform2609.literature_entity_lut WHERE {filter}) as fc
        SELECT CAST(c, 'UInt64') AS count,
            CAST(ly, 'UInt32') AS earliestPubYear,
            CAST(fc, 'UInt64') AS filteredCount,
            CAST(?, 'UInt64') as key,
            arraySlice(
                arrayReverseSort(
                    p->(p.relevance, p.date),
                    groupArray(
                        CAST(
                            tuple(pmid, pmcid, date, if(equals(year, 0), 1970, year), month, relevance),
                            'Tuple(pmid String, pmcid Nullable(String), date String, year UInt16, month UInt8, relevance Float64)'
                        )
                    )
                ),
                ?,
                ?
            ) AS rows
        FROM platform2609.literature_entity_lut
        WHERE {filter}");
    query
}

type IdMap = HashMap<u64, (Vec<String>, Option<u32>, Option<u32>, Page)>;

type ResultMap = HashMap<(Vec<String>, Option<u32>, Option<u32>, Page), Publications>;

impl Loader<(Vec<String>, Option<u32>, Option<u32>, Page)> for PublicationLoader {
    type Value = Publications;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[(Vec<String>, Option<u32>, Option<u32>, Page)],
    ) -> Result<HashMap<(Vec<String>, Option<u32>, Option<u32>, Page), Self::Value>, Self::Error>
    {
        let ids: IdMap = keys
            .iter()
            .enumerate()
            .map(|(pos, val)| (pos as u64, val.clone()))
            .collect();
        let base_queries: Vec<String> = ids.iter().map(build_query).collect();

        let full_query = ids.iter().fold(
            self.ch.query(&base_queries.join(" UNION ALL ")),
            |acc: clickhouse::query::Query, key| {
                acc.bind(key.1.1.unwrap_or(0))
                    .bind(key.1.2.unwrap_or(0))
                    .bind(key.1.0.clone())
                    .bind(key.0)
                    .bind((key.1.3.index * key.1.3.size) + 1)
                    .bind(key.1.3.size)
            },
        );

        let rows: Vec<Publications> = full_query.fetch_all().await?;

        let mut results: ResultMap = HashMap::new();

        for row in rows {
            let id = ids.clone().get(&row.key).unwrap().clone();
            results.entry(id.clone()).or_insert(row);
        }
        Ok(results)
    }
}

fn parse_date(year: Option<u32>, month: Option<u32>) -> Option<u32> {
    match (year, month) {
        (Some(year), Some(month)) => Some((year * 100) + month),
        (None, Some(month)) => Some(month),
        (Some(year), None) => Some(year * 100),
        (None, None) => None,
    }
}

pub struct PublicationsArg {
    pub ids: Vec<String>,
    pub start_year: Option<u32>,
    pub start_month: Option<u32>,
    pub end_year: Option<u32>,
    pub end_month: Option<u32>,
    pub page: Page,
}

/// Loads Publications by the target id from the cache or database.
///
/// # Returns
/// A `Vec` of `Publication` objects corresponding to the given target IDs.
/// # Errors
/// Returns an error if the Publications could not be loaded.
pub async fn load_paged_publications_by_keyword_id_date(
    ctx: &Context<'_>,
    args: PublicationsArg,
) -> async_graphql::Result<Option<Publications>, async_graphql::Error> {
    let start: Option<u32> = parse_date(args.start_year, args.start_month);
    let end: Option<u32> = parse_date(args.end_year, args.end_month);
    ctx.data_unchecked::<DataLoader<PublicationLoader>>()
        .load_one((args.ids, start, end, args.page).clone())
        .await
}
