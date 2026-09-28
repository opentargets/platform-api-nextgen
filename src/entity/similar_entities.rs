use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, SimpleObject, Union,
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

/// Semantic similarity score between labels, used to suggest related entities.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Similarity {
    /// Entity category this similarity refers to (e.g., target, disease, drug).
    category: String,
    /// Identifier of the similar entity (e.g., Ensembl, EFO, ChEMBL ID).
    #[graphql(name = "id")]
    word: String,
    /// Similarity score between this entity and the query label. Scores are normalised between 0
    /// and 1; higher scores indicate more similar entities.
    #[graphql(name = "score")]
    similarity: f64,
}

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct SimilarEntityRow {
    id_idx: u64,
    total: u64,
    pub similarities: Vec<Similarity>,
}

#[derive(Union)]
pub enum SimilarityUnion {
    Target(Target),
    Drug(Drug),
    Disease(Disease),
}

// ---- loaders ----

/// The key for filtering timeseries.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct SimilarEntityKey {
    id: String,
    ids: Vec<String>,
    categories: Vec<String>,
    threshold: u64,
    page: Page,
}

fn build_query(entities: &[String]) -> String {
    let category_filter = match entities.len() {
        0 => "true",
        _ => "in(category,entities)",
    };
    format!(
        "(WITH ? as ids,
    ? as id,
    ? as entities,
    ? as threshold,
    ? as key_idx,
    (
        SELECT sumForEach(vector)
        FROM platform2609.ml_w2v PREWHERE (in(word,ids))
    ) AS vv,
    (
        SELECT count(*)
        FROM platform2609.ml_w2v PREWHERE (equals(word, id))
    ) AS total,
    sqrt(arraySum(x->x * x, vv)) AS vvnorm,
    if(
        and(notEquals(vvnorm, 0.0), notEquals(norm, 0.0)),
        divide(
            arraySum(x->x.1 * x.2, arrayZip(vv, vector)),
            multiply(norm, vvnorm)
        ),
        0.0
    ) AS similarity,
    (SELECT groupArray((category,
        word,
        similarity))
    FROM platform2609.ml_w2v
    PREWHERE total>0
    AND ({category_filter})
    WHERE (greaterOrEquals(similarity, threshold))) as similarities
    select CAST(key_idx, 'UInt64'), length(similarities), arraySlice(similarities,?,?))"
    )
}

impl Loader<SimilarEntityKey> for SimilarEntityLoader {
    type Value = SimilarEntityRow;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[SimilarEntityKey],
    ) -> Result<HashMap<SimilarEntityKey, Self::Value>, Self::Error> {
        let mut queries: Vec<String> = Vec::new();
        let mut ids: HashMap<usize, SimilarEntityKey> = HashMap::new();
        keys.iter().enumerate().for_each(|(idx, key)| {
            queries.push(build_query(&key.categories));
            ids.insert(idx, key.clone());
        });

        let full_query = ids.iter().fold(
            self.ch.query(&queries.join(" UNION ALL ")),
            |acc: clickhouse::query::Query, (i, key)| {
                let mut full_ids: Vec<String> = key.ids.clone();
                full_ids.push(key.id.clone());
                acc.bind(&full_ids)
                    .bind(&key.id)
                    .bind(&key.categories)
                    .bind(f64::from_bits(key.threshold))
                    .bind(i)
                    .bind((key.page.index * key.page.size) + 1)
                    .bind(key.page.size)
            },
        );

        let result = full_query.fetch_all::<SimilarEntityRow>().await?;
        let results_with_keys = result
            .iter()
            .map(|res| {
                let k: usize = usize::try_from(res.id_idx).unwrap();
                (ids.get(&k).unwrap().clone(), res.clone())
            })
            .collect();

        Ok(results_with_keys)
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
        let result = match self.category.as_str() {
            "drug" => load_drug(ctx, self.word.clone())
                .await?
                .map(SimilarityUnion::Drug),
            "target" => load_target(ctx, self.word.clone())
                .await?
                .map(SimilarityUnion::Target),
            "disease" => load_disease(ctx, self.word.clone())
                .await?
                .map(SimilarityUnion::Disease),
            _ => None,
        };
        Ok(result)
    }
}
