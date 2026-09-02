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
pub struct DrugWarningReference {
    id: String,
    source: String,
    url: String,
}

/// Blackbox and withdrawn information for drugs molecules included in ChEMBL database.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DrugWarning {
    /// Classification of toxicity type associated with the drug.
    toxicity_class: Option<String>,
    /// List of Open Targets molecule identifiers.
    chembl_ids: Vec<String>,
    /// Country where the warning was issued.
    country: Option<String>,
    /// Description of the drug adverse effect.
    description: Option<String>,
    /// Internal identifier for the drug warning record.
    id: Option<i64>,
    /// List of sources supporting the warning information.
    references: Vec<DrugWarningReference>,
    /// Classification of action taken (drug is withdrawn or has a black box warning).
    warning_type: String,
    /// Year when the warning was issued.
    year: Option<i32>,
    /// List of disease labels.
    efo_term: Option<String>,
    /// List of disease identifiers.
    efo_id: Option<String>,
    /// Disease identifier categorising the type of warning.
    efo_id_for_warning_class: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DrugWarnings {
    chembl_id: String,
    drug_warnings: Vec<DrugWarning>,
}

// ---- loaders ----

pub type DrugWarningCache = Cache<String, Option<DrugWarning>>;
static DRUG_CACHE: LazyLock<DrugWarningCache> = LazyLock::new(entity_cache);

pub struct DrugWarningLoader {
    ch: ClickHouse,
}

impl DrugWarningLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}
