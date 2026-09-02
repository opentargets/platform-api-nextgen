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
pub struct DrugWarning {
    toxicityClass: Option<String>,
    chemblIds: Vec<String>,
    country: Option<String>,
    description: Option<String>,
    id: Option<i64>,
    references: Vec<DrugWarningReference>,
    warningType: String,
    year: Option<i32>,
    efoTerm: Option<String>,
    efoId: Option<String>,
    efoIdForWarningClass: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
pub struct DrugWarnings {
    chemblId: String,
    drugWarnings: Vec<DrugWarning>,
}
