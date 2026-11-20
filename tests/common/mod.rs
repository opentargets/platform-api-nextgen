pub mod sample_data;

use std::{fmt::Write, sync::Arc};

use bytes::Bytes;
use clickhouse::Client;
use platform_api::{AppState, config::Config, server::http};
use reqwest::Response;
use serde_json::{Value, json};
use testcontainers::{
    ContainerAsync, CopyDataSource, CopyTargetOptions, GenericImage, ImageExt,
    core::{ContainerPort, IntoContainerPort, WaitFor, wait::HttpWaitStrategy},
    runners::AsyncRunner,
};
use tokio::{net::TcpListener, sync::OnceCell};

/// The data release the sample data belongs to.
///
/// The fixtures under `tests/fixtures/data` are a subset of the `platform-2612-3`
/// release, so the namespace the API derives from it is `platform2612`.
pub const DATA_RELEASE: &str = "26.12";

/// The schema of the `targets` ClickHouse table.
const TARGETS_SCHEMA: &str = include_str!("schema/targets.sql");

/// The mappings and settings of the search indices.
///
/// These are the production mappings, taken from `platform-output-support`, so the
/// tests exercise the same analysis chain as the deployed API.
const SEARCH_INDEX_SCHEMA: &str = include_str!("schema/search_index.json");

const CLICKHOUSE_IMAGE: (&str, &str) = ("clickhouse/clickhouse-server", "26.8.2.7");
const OPENSEARCH_IMAGE: (&str, &str) = ("opensearchproject/opensearch", "2.19.1");

/// The user configuration of the ClickHouse container.
///
/// This is the production configuration, copied from `platform-output-support`.
const CLICKHOUSE_USERS_XML: &str = include_str!("schema/users.xml");

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
            // The container is given the production user configuration. The
            // ClickHouse entrypoint only writes its own user configuration when
            // the user has not already been set through `users.d`, so copying
            // the file in leaves the entrypoint alone and the container ends up
            // with the same setup as the deployed one: a passwordless
            // `default` user reachable from anywhere.
            .with_copy_to(
                CopyTargetOptions::new("/etc/clickhouse-server/users.d/users.xml"),
                CopyDataSource::Data(CLICKHOUSE_USERS_XML.as_bytes().to_vec()),
            )
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

/// Creates the ClickHouse database and loads the sample targets into it.
async fn seed_clickhouse(url: &str, namespace: &str) {
    let client = Client::default().with_url(url);
    client
        .query(&format!("CREATE DATABASE {namespace}"))
        .execute()
        .await
        .expect("create test ClickHouse database");
    client
        .query(&TARGETS_SCHEMA.replace("{namespace}", namespace))
        .execute()
        .await
        .expect("create targets fixture table");

    // The sample data is loaded as JSON: the Parquet format of the fixtures uses
    // `large_list` and `large_string` types that the ClickHouse version used
    // here cannot read, and its JSON reader does not support the tuple shapes
    // the API expects.
    let targets = sample_data::targets();
    let body = targets
        .iter()
        .map(|target| serde_json::to_string(target).expect("serialize target"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut insert = client.insert_formatted_with(format!(
        "INSERT INTO {namespace}.targets FORMAT JSONEachRow"
    ));
    insert
        .send(Bytes::from(body))
        .await
        .expect("seed targets fixture table");
    insert.end().await.expect("finish seeding targets");
}

/// Creates the search indices and loads the sample search documents into them.
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
            .header("Content-Type", "application/json")
            .body(SEARCH_INDEX_SCHEMA)
            .send()
            .await
            .expect("create OpenSearch fixture index");
        assert!(
            response.status().is_success(),
            "create OpenSearch index failed: {}",
            response.text().await.unwrap_or_default()
        );
    }

    // The search documents of the sample data.
    let mut bulk = String::new();
    for document in sample_data::search_targets() {
        let id = document["id"].as_str().expect("search document has an id");
        writeln!(
            bulk,
            r#"{{"index":{{"_index":"{namespace}_search_target","_id":"{id}"}}}}"#
        )
        .expect("write search document action");
        bulk.push_str(&serde_json::to_string(&document).expect("serialize search document"));
        bulk.push('\n');
    }
    let response = client
        .post(format!("{url}/_bulk"))
        .header("Content-Type", "application/x-ndjson")
        .body(bulk)
        .send()
        .await
        .expect("index OpenSearch target fixtures");
    assert!(
        response.status().is_success(),
        "index OpenSearch target fixtures failed: {}",
        response.text().await.unwrap_or_default()
    );

    // The sample search documents and the sample targets are disjoint, so a
    // document is synthesised for one of the targets. This is what lets the
    // tests check that a search hit resolves to the target stored in
    // ClickHouse.
    let target_id = sample_data::first_target_id();
    let symbol = sample_data::first_target_symbol();
    let name = sample_data::first_target_name();
    let document = json!({
        "id": target_id,
        "name": name,
        "entity": "target",
        "category": ["Protein kinase"],
        "description": "Synthesised from the target view of the sample data",
        "keywords": [symbol, name],
        "prefixes": [symbol],
        "ngrams": symbol.chars().scan(String::new(), |acc, character| {
            acc.push(character);
            Some(acc.clone())
        }).collect::<Vec<_>>(),
        "terms": [name],
        "terms5": [name],
        "terms25": [name],
        "multiplier": 1.0
    });
    let response = client
        .post(format!("{url}/{namespace}_search_target/_doc/{target_id}"))
        .json(&document)
        .send()
        .await
        .expect("index OpenSearch target fixture");
    assert!(
        response.status().is_success(),
        "index OpenSearch target fixture failed: {}",
        response.text().await.unwrap_or_default()
    );

    // The production mappings disable the automatic refresh, so the indices are
    // refreshed explicitly.
    for index in indices {
        let response = client
            .post(format!("{url}/{namespace}_{index}/_refresh"))
            .send()
            .await
            .expect("refresh OpenSearch fixture index");
        assert!(response.status().is_success());
    }
}

/// Returns the GraphQL errors of a response, panicking with the payload when there are any.
pub fn graphql_errors(response: &Value) -> &[Value] {
    let errors = response["errors"]
        .as_array()
        .map_or(&[][..], |errors| errors.as_slice());
    assert!(errors.is_empty(), "unexpected GraphQL errors: {response}");
    errors
}
