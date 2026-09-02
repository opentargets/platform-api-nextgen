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

#[derive(Debug, Clone, Deserialize, SimpleObject)]
pub struct DrugWarningReference {
    id: String,
    source: String,
    url: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DrugWarning {
    toxicity_class: Option<String>,
    chembl_ids: Vec<String>,
    country: Option<String>,
    description: Option<String>,
    id: Option<i64>,
    references: Vec<DrugWarningReference>,
    warning_type: String,
    year: Option<i32>,
    efo_term: Option<String>,
    efo_id: Option<String>,
    efo_id_for_warning_class: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DrugWarnings {
    chembl_id: String,
    drug_warnings: Vec<DrugWarning>,
}
