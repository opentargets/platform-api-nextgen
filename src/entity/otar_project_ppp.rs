use std::collections::HashMap;

use async_graphql::{
    Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::datasource::clickhouse::ClickHouse;

// ---- models ----

#[derive(Debug, Clone, Row, Deserialize)]
pub struct OtarProjects {
    /// Disease identifier studied by a list OTAR projects.
    efo_id: String,
    /// List of OTAR projects studying a given disease.
    projects: Vec<OtarProject>,
}

/// Open Targets (OTAR) project information associated with a disease. Data only available in
/// Partner Platform Preview (PPP).
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct OtarProject {
    /// OTAR project code identifier.
    otar_code: String,
    /// Status of the OTAR project.
    status: Option<String>,
    #[allow(clippy::struct_field_names)]
    /// Name of the OTAR project.
    project_name: Option<String>,
    /// Reference or citation for the OTAR project.
    reference: String,
    /// Whether the project integrates data in the Open Targets Partner Preview (PPP).
    #[graphql(name = "integratesInPPP")]
    #[serde(rename = "integrates_data_PPP")]
    integrates_data_ppp: Option<bool>,
}

// ---- loaders ----

#[derive(From)]
pub struct OtarProjectsLoader {
    ch: ClickHouse,
}

impl Loader<String> for OtarProjectsLoader {
    type Value = Vec<OtarProject>;
    type Error = async_graphql::Error;

    async fn load(&self, key: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        let rows: Vec<OtarProjects> = self
            .ch
            .query("SELECT ?fields FROM otar_projects WHERE efo_id IN ?")
            .bind(key)
            .fetch_all()
            .await?;
        let mut results: HashMap<String, Vec<OtarProject>> = HashMap::new();
        for row in rows {
            results.entry(row.efo_id.clone()).or_insert(row.projects);
        }
        Ok(results)
    }
}

/// Loads OTAR Projects by the disease id from the cache or database.
///
/// # Returns
/// A `Vec` of `OtarProjects` objects corresponding to the given disease IDs.
/// # Errors
/// Returns an error if the OTAR Projects could not be loaded.
pub async fn load_otar_projects(
    ctx: &Context<'_>,
    id: String,
) -> async_graphql::Result<Vec<OtarProject>> {
    Ok(ctx
        .data_unchecked::<DataLoader<OtarProjectsLoader>>()
        .load_one(id.clone())
        .await?
        .unwrap_or_default())
}
