use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, Enum, SimpleObject, Union,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        disease::{Disease, load_disease},
        drug::{Drug, load_drug},
        target::{Target, load_target},
    },
    query::paginate::{Page, Paged},
};

// ---- models ----

/// Entity category this similarity refers to (`Target`, `Disease` or `Drug`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Enum)]
#[serde(try_from = "String")]
pub enum Category {
    /// The similarity refers to a `Target` entity.
    Target,
    /// The similarity refers to a `Disease` entity.
    Disease,
    /// The similarity refers to a `Drug` entity.
    Drug,
}

/// The union of `Target`, `Disease` and `Drug` entities.
#[derive(Union)]
#[allow(clippy::large_enum_variant)]
pub enum SimilarityUnion {
    /// The similarity refers to a `Target` entity.
    Target(Target),
    /// The similarity refers to a `Drug` entity.
    Drug(Drug),
    /// The similarity refers to a `Disease` entity.
    Disease(Disease),
}

impl TryFrom<String> for Category {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.as_str() {
            "target" => Ok(Self::Target),
            "drug" => Ok(Self::Drug),
            "disease" => Ok(Self::Disease),
            _ => Err(format!("unknown category: {s}")),
        }
    }
}

/// Semantic similarity score between labels, used to suggest related entities.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Similarity {
    /// Entity category this similarity refers to (`Target`, `Disease` or `Drug`).
    category: Category,
    /// Identifier of the similar entity (e.g., Ensembl, EFO, ChEMBL ID).
    #[graphql(name = "id")]
    word: String,
    /// Similarity score between this entity and the query label. Scores are normalised between 0
    /// and 1; higher scores indicate more similar entities.
    #[graphql(name = "score")]
    #[allow(clippy::struct_field_names)]
    similarity: f64,
}

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct SimilarEntityRow {
    id_idx: u64,
    total: u64,
    pub similarities: Vec<Similarity>,
}

// ---- loaders ----

/// The key for filtering similar entities.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct SimilarEntityKey {
    id: String,
    ids: Vec<String>,
    categories: Vec<String>,
    threshold: u64,
    page: Page,
}

impl Loader<SimilarEntityKey> for SimilarEntityLoader {
    type Value = SimilarEntityRow;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[SimilarEntityKey],
    ) -> Result<HashMap<SimilarEntityKey, Self::Value>, Self::Error> {
        const QUERY: &str = r"
        WITH
            ? AS ids,
            ? AS id,
            CAST(?, 'Array(String)') AS entities,
            ? AS threshold,
            ? AS key_idx,
            (SELECT sumForEach(vector) FROM ml_w2v PREWHERE word IN ids) AS vv,
            (SELECT count() FROM ml_w2v PREWHERE word = id) AS total,
            sqrt(arraySum(x -> x * x, vv)) AS vvnorm,
            if(
                vvnorm != 0 AND norm != 0,
                arraySum(x -> x.1 * x.2, arrayZip(vv, vector)) / (norm * vvnorm),
                0.0
            ) AS similarity
        SELECT
            CAST(key_idx, 'UInt64'),
            count(),
            arraySlice(arraySort(x -> (-x.3, x.2), groupArray((category, word, similarity))), ?, ?)
        FROM ml_w2v
        PREWHERE total > 0 AND (empty(entities) OR has(entities, category))
        WHERE similarity >= threshold";

        let sql = vec![QUERY; keys.len()].join(" UNION ALL ");

        let query = keys
            .iter()
            .enumerate()
            .fold(self.ch.query(&sql), |q, (i, key)| {
                let mut all_ids = key.ids.clone();
                all_ids.push(key.id.clone());
                q.bind(all_ids)
                    .bind(&key.id)
                    .bind(&key.categories)
                    .bind(f64::from_bits(key.threshold))
                    .bind(i as u64)
                    .bind(key.page.index * key.page.size + 1)
                    .bind(key.page.size)
            });

        let rows = query.fetch_all::<SimilarEntityRow>().await?;

        rows.into_iter()
            .map(|row| {
                #[allow(clippy::cast_possible_truncation)]
                let key = keys
                    .get(row.id_idx as usize)
                    .ok_or_else(|| async_graphql::Error::new("bad id_idx"))?;
                Ok((key.clone(), row))
            })
            .collect()
    }
}

#[derive(From)]
pub struct SimilarEntityLoader {
    ch: ClickHouse,
}

// ---- resolvers ----

pub struct SimilarEntityArguments {
    pub id: String,
    pub ids: Vec<String>,
    pub categories: Vec<String>,
    pub threshold: f64,
    pub page: Page,
}

/// Load the similar entities for a set of ids.
///
/// # Errors
/// Returns an [`async_graphql::Error`] if the database query fails.
pub async fn load_similar_entities(
    ctx: &Context<'_>,
    args: SimilarEntityArguments,
) -> async_graphql::Result<Paged<Similarity>> {
    let key = SimilarEntityKey {
        id: args.id.clone(),
        ids: args.ids.clone(),
        categories: args.categories.clone(),
        threshold: args.threshold.to_bits(),
        page: args.page,
    };
    let test = ctx
        .data_unchecked::<DataLoader<SimilarEntityLoader>>()
        .load_one(key)
        .await?
        .map(|r| Paged {
            count: r.total,
            rows: r.similarities,
        })
        .unwrap_or_default();
    Ok(test)
}

#[ComplexObject]
impl Similarity {
    /// Resolved Platform entity corresponding to this similar label.
    async fn object(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<SimilarityUnion>, async_graphql::Error> {
        let id = self.word.clone();
        Ok(match self.category {
            Category::Target => load_target(ctx, id).await?.map(Into::into),
            Category::Disease => load_disease(ctx, id).await?.map(Into::into),
            Category::Drug => load_drug(ctx, id).await?.map(Into::into),
        })
    }
}
