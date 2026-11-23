//! Runs every `tests/live/<product>/*.graphql` against a live ClickHouse database and asserts
//! that none returns a GraphQL error.
//!
//! ```sh
//! LIVE_CLICKHOUSE_URL=http://localhost:8123 \
//!     cargo test --features product-platform --test live_clickhouse -- --ignored
//! ```
//!
//! A query file may start with `# variables: {...}`. With `LIVE_FIXTURES=1`, each
//! `tests/live/fixtures/<table>.jsonl` replaces the rows of `<table>` first: scratch databases
//! only.

use std::{
    fs,
    path::{Path, PathBuf},
};

use async_graphql::{PathSegment, Request, Variables};
use axum::body::Bytes;
use figment::{
    Figment,
    providers::{Format, Toml},
};
use platform_api::{
    AppState,
    config::Config,
    datasource::clickhouse::ClickHouse,
    product::{CurrentProduct, Product},
};

const LIVE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/live");

fn config(clickhouse_url: &str) -> Config {
    let release: Vec<&str> = env!("CARGO_PKG_VERSION").split('.').take(2).collect();
    let toml = format!(
        "data_release = \"{}\"\nclickhouse_url = {clickhouse_url:?}\n\
         opensearch_url = \"http://localhost:9200\"\nmax_depth = 64\n\
         max_complexity = 1_000_000_000\n",
        release.join(".")
    );
    Figment::from(Toml::string(&toml))
        .extract()
        .expect("live test config")
}

/// Replaces the rows of `table` with the `JSONEachRow` lines of `jsonl_path`.
///
/// # Panics
/// Panics if the file cannot be read or ClickHouse rejects the rows.
pub async fn insert_fixture(ch: &ClickHouse, table: &str, jsonl_path: &Path) {
    let rows = fs::read(jsonl_path).unwrap_or_else(|e| panic!("{}: {e}", jsonl_path.display()));
    ch.client()
        .query(&format!("TRUNCATE TABLE `{table}`"))
        .execute()
        .await
        .unwrap_or_else(|e| panic!("truncate {table}: {e}"));
    let mut insert = ch
        .client()
        .insert_formatted_with(format!("INSERT INTO `{table}` FORMAT JSONEachRow"));
    insert
        .send(Bytes::from(rows))
        .await
        .unwrap_or_else(|e| panic!("insert {}: {e}", jsonl_path.display()));
    insert
        .end()
        .await
        .unwrap_or_else(|e| panic!("insert {}: {e}", jsonl_path.display()));
}

fn files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == extension))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// The query and its variables, read from a `# variables: {...}` first line.
fn request(path: &Path) -> Request {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let variables = text
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("# variables:"))
        .map(|json| {
            serde_json::from_str(json)
                .unwrap_or_else(|e| panic!("{}: bad variables line: {e}", path.display()))
        });
    let request = Request::new(text.clone());
    match variables {
        Some(json) => request.variables(Variables::from_json(json)),
        None => request,
    }
}

#[tokio::test]
#[ignore = "needs a ClickHouse database: set LIVE_CLICKHOUSE_URL"]
async fn live_queries_return_no_errors() {
    let url = std::env::var("LIVE_CLICKHOUSE_URL").expect("set LIVE_CLICKHOUSE_URL");
    let config = config(&url);
    let ch = ClickHouse::new(&config);

    if std::env::var("LIVE_FIXTURES").is_ok_and(|v| v == "1") {
        for path in files(&Path::new(LIVE).join("fixtures"), "jsonl") {
            let table = path.file_stem().and_then(|s| s.to_str()).expect("table");
            insert_fixture(&ch, table, &path).await;
        }
    }

    let state = AppState::new(config);
    let product = <CurrentProduct as Product>::NAME.to_string();
    let queries = files(&Path::new(LIVE).join(&product), "graphql");
    assert!(!queries.is_empty(), "no queries in tests/live/{product}");

    let mut errors = Vec::new();
    for path in &queries {
        let response = state.api.execute(request(path)).await;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        for error in &response.errors {
            let at: Vec<String> = error
                .path
                .iter()
                .map(|s| match s {
                    PathSegment::Field(f) => f.clone(),
                    PathSegment::Index(i) => i.to_string(),
                })
                .collect();
            errors.push(format!("{name}: {}: {}", at.join("."), error.message));
        }
    }
    errors.sort();
    assert!(
        errors.is_empty(),
        "{} GraphQL errors in {} query files:\n{}",
        errors.len(),
        queries.len(),
        errors.join("\n")
    );
}
