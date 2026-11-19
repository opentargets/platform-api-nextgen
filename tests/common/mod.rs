use std::sync::Arc;

use clickhouse::Client;
use platform_api::{AppState, config::Config, server::http};
use reqwest::Response;
use serde_json::{Value, json};
use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{ContainerPort, IntoContainerPort, WaitFor, wait::HttpWaitStrategy},
    runners::AsyncRunner,
};
use tokio::{net::TcpListener, sync::OnceCell};

pub const DATA_RELEASE: &str = "26.09";
pub const TARGET_ID: &str = "ENSG00000139618";
pub const TARGET_SYMBOL: &str = "BRCA2";
pub const TARGET_NAME: &str = "Breast cancer type 2 susceptibility protein";
pub const OTHER_TARGET_ID: &str = "ENSG00000141510";
pub const OTHER_TARGET_SYMBOL: &str = "TP53";

const CLICKHOUSE_IMAGE: (&str, &str) = ("clickhouse/clickhouse-server", "23.3.8.21-alpine");
const OPENSEARCH_IMAGE: (&str, &str) = ("opensearchproject/opensearch", "2.19.1");

/// A running API backed by real ClickHouse and OpenSearch containers.
///
/// The whole suite shares one environment: `axum-prometheus` installs a global metrics
/// recorder, which can only be set once per process.
pub struct TestEnvironment {
    pub graphql_url: String,
    _clickhouse: ContainerAsync<GenericImage>,
    _opensearch: ContainerAsync<GenericImage>,
}

static ENVIRONMENT: OnceCell<Arc<TestEnvironment>> = OnceCell::const_new();

/// Returns the shared test environment, starting it on first use.
///
/// The environment (containers plus the HTTP server) runs on its own thread and runtime,
/// so that it outlives the per-test runtimes: every `#[tokio::test]` gets a runtime that
/// is dropped when the test ends, which would take the server and the containers with it.
pub async fn environment() -> Arc<TestEnvironment> {
    ENVIRONMENT
        .get_or_init(|| async {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("build shared test runtime");
                runtime.block_on(async move {
                    let environment = Arc::new(TestEnvironment::start().await);
                    let _ = sender.send(environment);
                    // Keep the runtime alive so the server keeps serving.
                    std::future::pending::<()>().await;
                });
            });
            receiver
                .await
                .expect("shared test environment failed to start")
        })
        .await
        .clone()
}

impl TestEnvironment {
    async fn start() -> Self {
        let clickhouse = GenericImage::new(CLICKHOUSE_IMAGE.0, CLICKHOUSE_IMAGE.1)
            .with_exposed_port(8123_u16.tcp())
            .with_wait_for(WaitFor::http(
                HttpWaitStrategy::new("/ping")
                    .with_port(8123_u16.tcp())
                    .with_expected_status_code(200_u16),
            ))
            .start()
            .await
            .expect("start ClickHouse container");
        let clickhouse_url = container_url(&clickhouse, 8123).await;

        let opensearch = GenericImage::new(OPENSEARCH_IMAGE.0, OPENSEARCH_IMAGE.1)
            .with_exposed_port(9200_u16.tcp())
            .with_wait_for(WaitFor::http(
                HttpWaitStrategy::new("/")
                    .with_port(9200_u16.tcp())
                    .with_expected_status_code(200_u16),
            ))
            .with_env_var("discovery.type", "single-node")
            .with_env_var("DISABLE_SECURITY_PLUGIN", "true")
            .start()
            .await
            .expect("start OpenSearch container");
        let opensearch_url = container_url(&opensearch, 9200).await;

        let config = Config::for_tests(DATA_RELEASE, &clickhouse_url, &opensearch_url);
        let namespace = config.data_namespace();
        seed_clickhouse(&clickhouse_url, &namespace).await;
        seed_opensearch(&opensearch_url, &namespace).await;

        let state = AppState::new(config);
        let (router, _) = http::router(state);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind GraphQL test server");
        let address = listener.local_addr().expect("read test server address");

        tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("serve GraphQL test server");
        });

        Self {
            graphql_url: format!("http://{address}/latest/graphql"),
            _clickhouse: clickhouse,
            _opensearch: opensearch,
        }
    }

    pub async fn graphql(&self, query: &str) -> Value {
        self.graphql_with_variables(query, &json!({})).await
    }

    pub async fn graphql_with_variables(&self, query: &str, variables: &Value) -> Value {
        let response = reqwest::Client::new()
            .post(&self.graphql_url)
            .json(&json!({ "query": query, "variables": variables }))
            .send()
            .await
            .expect("send GraphQL request");
        assert_eq!(response.status(), 200, "GraphQL request failed");
        response_json(response).await
    }
}

async fn container_url<I>(container: &ContainerAsync<I>, port: u16) -> String
where
    I: testcontainers::Image,
{
    let host = container.get_host().await.expect("read container host");
    let port = container
        .get_host_port_ipv4(ContainerPort::Tcp(port))
        .await
        .expect("read mapped container port");
    format!("http://{host}:{port}")
}

async fn response_json(response: Response) -> Value {
    let body = response.text().await.expect("read GraphQL response");
    serde_json::from_str(&body)
        .unwrap_or_else(|error| panic!("invalid GraphQL response: {error}: {body}"))
}

async fn seed_clickhouse(url: &str, namespace: &str) {
    let client = Client::default().with_url(url);
    client
        .query(&format!("CREATE DATABASE {namespace}"))
        .execute()
        .await
        .expect("create test ClickHouse database");
    client
        .query(&format!(
            r"
            CREATE TABLE {namespace}.targets (
                `id` String,
                `alternativeGenes` Array(String),
                `approvedSymbol` String,
                `approvedName` String,
                `biotype` String,
                `canonicalTranscript` Tuple(String, String, Int32, Int32, Int8),
                `chemicalProbes` Array(Tuple(
                    String, Nullable(String), Nullable(String), Nullable(String),
                    Array(String), Bool, Array(String), Nullable(Float64),
                    Nullable(Float64), Nullable(Float64), String,
                    Array(Tuple(String, Nullable(String)))
                )),
                `dbXrefs` Array(Tuple(String, String)),
                `functionDescriptions` Array(String),
                `constraint` Array(Tuple(
                    String, Nullable(Float64), Nullable(UInt32), Nullable(Float64),
                    Nullable(Float64), Nullable(Float64), Nullable(Float64),
                    Nullable(UInt32), Nullable(UInt32), Nullable(UInt32)
                )),
                `genomicLocation` Tuple(String, UInt32, UInt32, Int8),
                `go` Array(Tuple(String, String, String, String, String)),
                `hallmarks` Tuple(
                    Array(Tuple(String, Nullable(String), String, UInt32)),
                    Array(Tuple(String, String, Nullable(UInt32)))
                ),
                `homologues` Array(Tuple(
                    String, Float64, String, String, String, String, Float64, Nullable(String)
                )),
                `pathways` Array(Tuple(String, String, String)),
                `proteinIds` Array(Tuple(String, String)),
                `safetyLiabilities` Array(Tuple(
                    Array(Tuple(Nullable(String), Nullable(String), Nullable(String), Nullable(String), Nullable(String))),
                    String,
                    Array(Tuple(String, String)),
                    Nullable(String), Nullable(String), Nullable(String), Nullable(String),
                    Array(Tuple(Nullable(String), Nullable(String), Nullable(String)))
                )),
                `studyLocusIds` Array(String),
                `pharmacogenomicsIds` Array(UInt64),
                `subcellularLocations` Array(Tuple(
                    String, String, Nullable(String), Nullable(String), Nullable(String)
                )),
                `synonyms` Array(Tuple(String, String)),
                `symbolSynonyms` Array(Tuple(String, String)),
                `nameSynonyms` Array(Tuple(String, String)),
                `obsoleteSymbols` Array(Tuple(String, String)),
                `obsoleteNames` Array(Tuple(String, String)),
                `targetClass` Array(Tuple(UInt32, String, String)),
                `tractability` Array(Tuple(String, String, Bool)),
                `transcriptIds` Array(String),
                `transcripts` Array(Tuple(
                    String, String, Bool, Nullable(String), Bool,
                    Nullable(String), Nullable(String), Nullable(String)
                ))
            ) ENGINE = Memory
            "
        ))
        .execute()
        .await
        .expect("create targets fixture table");
    for (id, symbol, name, transcript, chromosome, start, end, strand) in [
        (
            TARGET_ID,
            TARGET_SYMBOL,
            TARGET_NAME,
            "ENST00000380152",
            "13",
            32_315_474_i32,
            32_398_710_i32,
            1_i8,
        ),
        (
            OTHER_TARGET_ID,
            OTHER_TARGET_SYMBOL,
            "Cellular tumor antigen p53",
            "ENST00000269305",
            "17",
            7_668_402_i32,
            7_687_550_i32,
            -1_i8,
        ),
    ] {
        client
            .query(&format!(
                r"
                INSERT INTO {namespace}.targets
                    (`id`, `alternativeGenes`, `approvedSymbol`, `approvedName`, `biotype`,
                     `canonicalTranscript`, `chemicalProbes`, `dbXrefs`, `functionDescriptions`,
                     `constraint`, `genomicLocation`, `go`, `hallmarks`, `homologues`, `pathways`,
                     `proteinIds`, `safetyLiabilities`, `studyLocusIds`, `pharmacogenomicsIds`,
                     `subcellularLocations`, `synonyms`, `symbolSynonyms`, `nameSynonyms`,
                     `obsoleteSymbols`, `obsoleteNames`, `targetClass`, `tractability`,
                     `transcriptIds`, `transcripts`)
                VALUES ('{id}', [], '{symbol}', '{name}', 'protein_coding', ('{transcript}', '{chromosome}', {start}, {end}, {strand}), [], [], [], [], ('{chromosome}', {start}, {end}, {strand}), [], ([], []), [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [('{transcript}', 'protein_coding', true, 'P00000', true, 'ENSP00000000000', 'AF-P00000-F1', CAST(NULL AS Nullable(String)))])
                "
            ))
            .execute()
            .await
            .expect("seed targets fixture table");
    }
}

async fn seed_opensearch(url: &str, namespace: &str) {
    let client = reqwest::Client::new();
    let indices = [
        "search_disease",
        "search_target",
        "search_drug",
        "search_study",
        "search_variant",
    ];
    for index in indices {
        let response = client
            .put(format!("{url}/{namespace}_{index}"))
            .json(&json!({
                "settings": {
                    "number_of_shards": 1,
                    "number_of_replicas": 0,
                    "analysis": {
                        "normalizer": {
                            "custom": {
                                "type": "custom",
                                "filter": ["lowercase"]
                            }
                        },
                        "char_filter": {
                            "normalize_delimiters": {
                                "type": "pattern_replace",
                                "pattern": "[:\\-/]",
                                "replacement": "_"
                            }
                        },
                        "analyzer": {
                            "token": {
                                "type": "custom",
                                "tokenizer": "keyword",
                                "char_filter": ["normalize_delimiters"],
                                "filter": ["lowercase", "trim"]
                            }
                        }
                    }
                },
                "mappings": {
                    "dynamic_templates": [{
                        "strings": {
                            "match_mapping_type": "string",
                            "mapping": {
                                "type": "text",
                                "term_vector": "with_positions_offsets",
                                "fields": {
                                    "raw": {
                                        "type": "keyword",
                                        "ignore_above": 1024,
                                        "normalizer": "custom"
                                    }
                                }
                            }
                        }
                    }]
                }
            }))
            .send()
            .await
            .expect("create OpenSearch fixture index");
        assert!(
            response.status().is_success(),
            "create OpenSearch index failed: {}",
            response.text().await.unwrap_or_default()
        );
    }

    let document = json!({
        "id": TARGET_ID,
        "name": TARGET_NAME,
        "entity": "target",
        "category": ["Protein kinase"],
        "description": "Homolog of the breast cancer type 2 susceptibility protein",
        "keywords": [TARGET_SYMBOL, "BRCA"],
        "prefixes": ["BRCA2"],
        "ngrams": ["B", "BR", "BRA", "BRCA", "BRCA2"],
        "terms": ["breast cancer type 2 susceptibility protein"],
        "terms5": ["breast cancer type 2 susceptibility protein"],
        "terms25": ["breast cancer type 2 susceptibility protein"],
        "multiplier": 1.0
    });
    let response = client
        .post(format!("{url}/{namespace}_search_target/_doc/{TARGET_ID}"))
        .json(&document)
        .send()
        .await
        .expect("index OpenSearch target fixture");
    assert!(
        response.status().is_success(),
        "index OpenSearch target fixture failed: {}",
        response.text().await.unwrap_or_default()
    );
    let response = client
        .post(format!("{url}/{namespace}_search_target/_refresh"))
        .send()
        .await
        .expect("refresh OpenSearch target fixture");
    assert!(response.status().is_success());
}

/// Returns the GraphQL errors of a response, panicking with the payload when there are any.
pub fn graphql_errors(response: &Value) -> &[Value] {
    let errors = response["errors"]
        .as_array()
        .map_or(&[][..], |errors| errors.as_slice());
    assert!(errors.is_empty(), "unexpected GraphQL errors: {response}");
    errors
}
