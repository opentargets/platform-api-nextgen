use std::{collections::HashMap, sync::LazyLock};

use async_graphql::{
    Context, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
    },
};

// ---- models ----

#[derive(Debug, Clone, Deserialize, SimpleObject)]
pub struct DrugReferences {
    source: String,
    ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
pub struct DrugLabelAndSource {
    label: String,
    source: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject, Row)]
#[serde(rename_all = "camelCase")]
pub struct Drug {
    id: String,
    name: String,
    synonyms: Vec<DrugLabelAndSource>,
    trade_names: Vec<DrugLabelAndSource>,
    #[allow(clippy::struct_field_names)]
    drug_type: String,
    cross_references: Vec<DrugReferences>,
    parent_id: Option<String>,
    maximum_clinical_stage: String,
    description: Option<String>,
    molblock: Option<String>,
}

// ---- query utilities ----

impl Entity for Drug {
    fn id(&self) -> &str { &self.id }
}

// ---- loaders ----

pub type DrugCache = Cache<String, Option<Drug>>;
static DRUG_CACHE: LazyLock<DrugCache> = LazyLock::new(entity_cache);

pub struct DrugLoader {
    ch: ClickHouse,
}

impl DrugLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl CachedLoader for DrugLoader {
    type Key = String;
    type Value = Drug;

    fn cache(&self) -> &DrugCache { &DRUG_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM drugs WHERE id IN ?")
            .bind(misses)
            .fetch_all::<Drug>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for DrugLoader {
    type Value = Drug;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[String]) -> Result<HashMap<String, Drug>, async_graphql::Error> {
        self.load_cached(keys).await
    }
}

#[allow(clippy::missing_errors_doc)]
pub async fn load_drugs(ctx: &Context<'_>, ids: &[String]) -> async_graphql::Result<Vec<Drug>> {
    load_ordered(ctx.data_unchecked::<DataLoader<DrugLoader>>(), ids).await
}

// ---- resolvers ----

#[derive(Default)]
pub struct DrugQuery;

#[Object]
impl DrugQuery {
    async fn drugs(
        &self,
        ctx: &Context<'_>,
        ensembl_ids: Vec<String>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<Drug>> {
        let drugs = load_drugs(ctx, &ensembl_ids).await?;
        Ok(drugs.query().paginate(page))
    }

    async fn drug(
        &self,
        ctx: &Context<'_>,
        target_id: String,
    ) -> async_graphql::Result<Option<Drug>> {
        ctx.data_unchecked::<DataLoader<DrugLoader>>()
            .load_one(target_id)
            .await
    }
}
