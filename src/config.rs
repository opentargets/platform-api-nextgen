//! Configuration definitions and loading logic.

use std::{
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

use clap::Parser;
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::{
    Deserialize, Deserializer,
    de::{Error, MapAccess, Visitor},
};

use crate::plugin::plugin::Plugin;

const DEFAULT_CONFIG_FILE: &str = "config.toml";
pub const DEFAULT_CACHE_CAPACITY: u64 = 100_000;

#[derive(Parser)]
pub struct Args {
    /// Path to the TOML configuration file.
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,
}

/// The configuration for the API.
#[derive(Deserialize, Debug)]
pub struct Config {
    /// The data release used by the API. Either `YY.MM` or `YY.MM.rev`.
    #[serde(deserialize_with = "release")]
    pub data_release: String,
    /// The product served by the API (platform/ppp).
    pub product: String,
    /// The log level to use. Defaults to `info`.
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// The address to bind the HTTP server to. Defaults to `0.0.0.0:8080`.
    #[serde(default = "default_bind_address")]
    pub bind_address: String,

    /// Database settings.
    /// The URL of the ClickHouse database.
    pub clickhouse_url: String,
    /// The maximum execution time for ClickHouse queries. Defaults to 0 (no limit).
    #[serde(default, with = "humantime_serde")]
    pub clickhouse_max_execution_time: Duration,
    /// The URL of the OpenSearch database.
    pub opensearch_url: String,
    /// The timeout for OpenSearch requests. Defaults to 10 seconds.
    #[serde(default = "default_opensearch_timeout", with = "humantime_serde")]
    pub opensearch_timeout: Duration,
    /// The maximum depth of the GraphQL schema. Defaults to 10.
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    /// The maximum complexity of the GraphQL schema. Defaults to 1000.
    #[serde(default = "default_max_complexity")]
    pub max_complexity: usize,

    /// The plugin list in the form `name=base_url,name=base_url`.
    #[serde(default, deserialize_with = "plugins")]
    pub plugins: Vec<Plugin>,
}

impl Config {
    /// Loads the configuration from environment variables and the config file.
    ///
    /// # Panics
    /// Panics if the configuration is invalid or if the config file is specified but does not
    /// exist.
    #[must_use]
    pub fn load(config_file: Option<&Path>) -> Self {
        let mut fig = Figment::new();

        match config_file {
            Some(path) => fig = fig.merge(Toml::file_exact(path)),
            None if Path::new(DEFAULT_CONFIG_FILE).exists() => {
                fig = fig.merge(Toml::file(DEFAULT_CONFIG_FILE));
            }
            None => {}
        }

        fig.merge(Env::prefixed("PLATFORM_API_"))
            .extract()
            .unwrap_or_else(|e| panic!("invalid configuration: {e}"))
    }

    /// The data namespace used in ClickHouse and OpenSearch.
    ///
    /// It is used to isolate releases/products to allow more than one in the same db instance.
    /// In the case of ClickHouse, the database name is the namespace. In OpenSearch, the indices
    /// are prefixed with the namespace.
    ///
    /// Formed by concatenating the product with the data release, e.g.: `platform2606` or `ppp2204`,
    /// where the trailing four digits are `YYMM`. Revision suffixes are stripped.
    #[must_use]
    pub fn data_namespace(&self) -> String {
        let release: String = self.data_release.split('.').take(2).collect();
        format!("{}{}", self.product, release)
    }

    /// The data release in the form `YY.MM`, without revision suffixes.
    #[must_use]
    pub fn data_release_main(&self) -> String { self.data_release.split('.').take(2).collect() }
}

fn default_log_level() -> String { "info".to_string() }
fn default_bind_address() -> String { "0.0.0.0:8080".to_string() }
fn default_opensearch_timeout() -> Duration { Duration::from_secs(10) }
fn default_max_depth() -> usize { 10 }
fn default_max_complexity() -> usize { 1000 }

fn is_num(s: &str) -> bool { s.chars().all(|c| c.is_ascii_digit()) }
fn is_year(yy: &str) -> bool { is_num(yy) && yy.len() == 2 }
fn is_month(m: &str) -> bool { is_num(m) && m.len() == 2 && matches!(m.parse::<u8>(), Ok(1..=12)) }

fn is_valid_release(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    match parts[..] {
        [yy, mm] => is_year(yy) && is_month(mm),
        [yy, mm, rev] => is_year(yy) && is_month(mm) && is_num(rev),
        _ => false,
    }
}

fn release<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if !is_valid_release(&s) {
        return Err(D::Error::custom(format!(
            "invalid data_release '{s}', expected YY.MM or YY.MM.rev"
        )));
    }
    Ok(s)
}

fn plugins<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Plugin>, D::Error> {
    struct PluginsVisitor;

    impl<'de> Visitor<'de> for PluginsVisitor {
        type Value = Vec<Plugin>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a `name=base_url,...` string or a table of name = base_url")
        }

        fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let (name, base_url) = s.split_once('=').ok_or_else(|| {
                        E::custom(format!("invalid plugin '{s}', expected name=base_url"))
                    })?;
                    Plugin::new(name.trim(), base_url.trim())
                        .map_err(|e| E::custom(format!("invalid plugin '{s}': {e:?}")))
                })
                .collect()
        }

        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut plugins = Vec::new();
            while let Some((name, base_url)) = map.next_entry::<String, String>()? {
                plugins.push(Plugin::new(&name, &base_url).map_err(|e| {
                    M::Error::custom(format!("invalid plugin '{name}={base_url}': {e:?}"))
                })?);
            }
            Ok(plugins)
        }
    }

    d.deserialize_any(PluginsVisitor)
}
