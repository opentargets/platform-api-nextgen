pub mod frontend_queries;
pub mod sample_data;

use std::{
    fmt::Write,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

use bytes::Bytes;
use clickhouse::Client;
use platform_api::{AppState, config::Config, server::http};
use reqwest::Response;
use serde_json::{Value, json};
use testcontainers::{
    ContainerAsync, CopyDataSource, CopyTargetOptions, GenericImage, ImageExt,
    bollard::models::HostConfig,
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

/// The schema of the `drug` ClickHouse table.
const DRUG_SCHEMA: &str = include_str!("schema/drug.sql");

/// The schema of the `disease` ClickHouse table.
const DISEASE_SCHEMA: &str = include_str!("schema/disease.sql");

/// The schema of the `study` ClickHouse table.
const STUDY_SCHEMA: &str = include_str!("schema/study.sql");

/// The schema of the `clinical_report` ClickHouse table.
const CLINICAL_REPORT_SCHEMA: &str = include_str!("schema/clinical_report.sql");

/// The schema of the `variants` ClickHouse table.
const VARIANTS_SCHEMA: &str = include_str!("schema/variants.sql");

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

/// Publishes the container ports on the loopback interface only.
///
/// Docker publishes the ports of a container on every interface of the host by
/// default. The containers hold an unauthenticated ClickHouse and an OpenSearch
/// with its security plugin disabled, so they must not be reachable from other
/// hosts.
fn publish_on_loopback_only(host_config: &mut HostConfig) {
    let Some(bindings) = host_config.port_bindings.as_mut() else {
        return;
    };
    for hosts in bindings.values_mut().flatten() {
        for host in hosts.iter_mut() {
            host.host_ip = Some("127.0.0.1".to_string());
        }
    }
}

/// A running API backed by real ClickHouse and OpenSearch containers.
///
/// The whole suite shares one environment: starting the containers takes about
/// twenty seconds, and every test needs the same data.
pub struct TestEnvironment {
    pub graphql_url: String,
}

/// The containers of the shared environment.
///
/// They are owned by the environment thread rather than by [`TestEnvironment`],
/// so that the thread can remove them explicitly when the process exits: the
/// environment is kept in a `static`, and Rust does not run destructors for
/// statics, so dropping it would never remove the containers.
struct Containers {
    clickhouse: ContainerAsync<GenericImage>,
    opensearch: ContainerAsync<GenericImage>,
}

impl Containers {
    /// Removes the containers.
    async fn remove(self) {
        let Self {
            clickhouse,
            opensearch,
        } = self;
        clickhouse
            .rm()
            .await
            .expect("remove the ClickHouse container");
        opensearch
            .rm()
            .await
            .expect("remove the OpenSearch container");
    }
}

static ENVIRONMENT: OnceCell<Arc<TestEnvironment>> = OnceCell::const_new();

/// The messages the environment thread receives.
enum Message {
    /// The process is exiting and the containers have to be removed.
    Shutdown,
}

/// The channel the environment thread listens on.
static ENVIRONMENT_CHANNEL: OnceLock<mpsc::Sender<Message>> = OnceLock::new();

/// The thread the environment runs on, so that the exit handler can wait for it.
static ENVIRONMENT_THREAD: OnceLock<Mutex<Option<std::thread::JoinHandle<()>>>> = OnceLock::new();

/// Whether the exit handler has already run.
static SHUTDOWN_DONE: AtomicBool = AtomicBool::new(false);

/// Removes the containers when the test binary exits.
///
/// The containers are owned by a value that is never dropped on its own: the
/// environment is kept in a `static`, and Rust does not run destructors for
/// statics. Without this handler the containers would outlive every test run.
///
/// The handler runs as the process exits, which is after all the tests have
/// finished. It has to wait for the environment thread to remove the
/// containers, because the process terminates as soon as it returns.
extern "C" fn remove_containers_on_exit() {
    if SHUTDOWN_DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Some(sender) = ENVIRONMENT_CHANNEL.get() {
        let _ = sender.send(Message::Shutdown);
    }
    if let Some(thread) = ENVIRONMENT_THREAD
        .get()
        .and_then(|slot| slot.lock().ok())
        .and_then(|mut slot| slot.take())
    {
        let _ = thread.join();
    }
}

/// Returns the shared test environment, starting it on first use.
///
/// The environment (containers plus the HTTP server) runs on its own thread and runtime,
/// so that it outlives the per-test runtimes: every `#[tokio::test]` gets a runtime that
/// is dropped when the test ends, which would take the server and the containers with it.
pub async fn environment() -> Arc<TestEnvironment> {
    ENVIRONMENT
        .get_or_init(|| async {
            let (shutdown_sender, shutdown_receiver) = mpsc::channel();
            let (ready_sender, ready_receiver) = mpsc::channel();
            let _ = ENVIRONMENT_CHANNEL.set(shutdown_sender);
            let thread = std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("build shared test runtime");
                runtime.block_on(async move {
                    let (environment, containers) = TestEnvironment::start().await;
                    let _ = ready_sender.send(environment);
                    // Keep serving until the exit handler asks for the teardown.
                    if let Ok(Message::Shutdown) = shutdown_receiver.recv() {
                        // The containers are removed explicitly: they cannot be
                        // removed by being dropped, because the environment they
                        // belong to is kept in a `static`.
                        containers.remove().await;
                    }
                });
            });
            let _ = ENVIRONMENT_THREAD.set(Mutex::new(Some(thread)));
            // SAFETY: the handler only sends a message to the environment thread and
            // joins it, both of which are safe to do while the process exits.
            unsafe { libc::atexit(remove_containers_on_exit) };
            ready_receiver
                .recv()
                .expect("shared test environment failed to start")
        })
        .await
        .clone()
}

impl TestEnvironment {
    /// Starts the containers, loads the sample data and serves the GraphQL API.
    ///
    /// Returns the environment the tests use, and the containers, which the
    /// environment thread keeps so that it can remove them when the process
    /// exits.
    async fn start() -> (Arc<Self>, Containers) {
        let clickhouse = GenericImage::new(CLICKHOUSE_IMAGE.0, CLICKHOUSE_IMAGE.1)
            .with_wait_for(WaitFor::http(
                HttpWaitStrategy::new("/ping")
                    .with_port(8123_u16.tcp())
                    .with_expected_status_code(200_u16),
            ))
            // The port is mapped explicitly, rather than exposed, so that it can be
            // published on the loopback interface only.
            .with_mapped_port(0, 8123_u16.tcp())
            .with_host_config_modifier(publish_on_loopback_only)
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
            .with_wait_for(WaitFor::http(
                HttpWaitStrategy::new("/")
                    .with_port(9200_u16.tcp())
                    .with_expected_status_code(200_u16),
            ))
            // The port is mapped explicitly, rather than exposed, so that it can be
            // published on the loopback interface only.
            .with_mapped_port(0, 9200_u16.tcp())
            .with_host_config_modifier(publish_on_loopback_only)
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

        (
            Arc::new(Self {
                graphql_url: format!("http://{address}/latest/graphql"),
            }),
            Containers {
                clickhouse,
                opensearch,
            },
        )
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

/// Creates the ClickHouse database and loads the sample targets, drugs and
/// diseases into it.
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
    client
        .query(&DRUG_SCHEMA.replace("{namespace}", namespace))
        .execute()
        .await
        .expect("create drug fixture table");
    client
        .query(&DISEASE_SCHEMA.replace("{namespace}", namespace))
        .execute()
        .await
        .expect("create disease fixture table");
    client
        .query(&STUDY_SCHEMA.replace("{namespace}", namespace))
        .execute()
        .await
        .expect("create study fixture table");
    client
        .query(&CLINICAL_REPORT_SCHEMA.replace("{namespace}", namespace))
        .execute()
        .await
        .expect("create clinical report fixture table");
    client
        .query(&VARIANTS_SCHEMA.replace("{namespace}", namespace))
        .execute()
        .await
        .expect("create variants fixture table");

    // The sample data is loaded as JSON: the Parquet format of the fixtures uses
    // `large_list` and `large_string` types that the ClickHouse version used
    // here cannot read, and its JSON reader does not support the tuple shapes
    // the API expects.
    insert_rows(&client, namespace, "targets", &sample_data::targets()).await;
    insert_rows(&client, namespace, "drug", &sample_data::drugs()).await;
    insert_rows(&client, namespace, "disease", &sample_data::diseases()).await;
    insert_rows(&client, namespace, "studies", &sample_data::studies()).await;
    insert_rows(
        &client,
        namespace,
        "clinical_report",
        &sample_data::clinical_reports(),
    )
    .await;
    insert_rows(&client, namespace, "variants", &sample_data::variants()).await;
}

/// Inserts rows into a table of the test database.
async fn insert_rows(client: &Client, namespace: &str, table: &str, rows: &[Value]) {
    let body = rows
        .iter()
        .map(|row| serde_json::to_string(row).expect("serialize sample data row"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut insert = client.insert_formatted_with(format!(
        "INSERT INTO {namespace}.{table} FORMAT JSONEachRow"
    ));
    insert
        .send(Bytes::from(body))
        .await
        .expect("seed sample data table");
    insert.end().await.expect("finish seeding sample data");
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
    index_search_documents(
        &client,
        url,
        namespace,
        "search_target",
        &sample_data::search_targets(),
    )
    .await;
    index_search_documents(
        &client,
        url,
        namespace,
        "search_drug",
        &sample_data::search_drugs(),
    )
    .await;
    index_search_documents(
        &client,
        url,
        namespace,
        "search_disease",
        &sample_data::search_diseases(),
    )
    .await;

    // The sample search documents and the sample entities are disjoint, so a
    // document is synthesised for one of the targets, one of the drugs and one
    // of the diseases. This is what lets the tests check that a search hit
    // resolves to the entity stored in ClickHouse.
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
        "ngrams": ngrams(&symbol),
        "terms": [name],
        "terms5": [name],
        "terms25": [name],
        "multiplier": 1.0
    });
    index_search_document(
        &client,
        url,
        namespace,
        "search_target",
        &target_id,
        &document,
    )
    .await;

    let drug_id = sample_data::first_drug_id();
    let drug_name = sample_data::first_drug_name();
    let document = json!({
        "id": drug_id,
        "name": drug_name,
        "entity": "drug",
        "category": ["Small molecule"],
        "description": "Synthesised from the drug molecule dataset of the sample data",
        "keywords": [drug_id, drug_name],
        "prefixes": [drug_name],
        "ngrams": ngrams(&drug_name),
        "terms": [drug_name],
        "terms5": [drug_name],
        "terms25": [drug_name],
        "multiplier": 1.0
    });
    index_search_document(&client, url, namespace, "search_drug", &drug_id, &document).await;

    let disease_id = sample_data::first_disease_id();
    let disease_name = sample_data::first_disease_name();
    let document = json!({
        "id": disease_id,
        "name": disease_name,
        "entity": "disease",
        "category": ["disease"],
        "description": "Synthesised from the disease dataset of the sample data",
        "keywords": [disease_id, disease_name],
        "prefixes": [disease_name],
        "ngrams": ngrams(&disease_name),
        "terms": [disease_name],
        "terms5": [disease_name],
        "terms25": [disease_name],
        "multiplier": 1.0
    });
    index_search_document(
        &client,
        url,
        namespace,
        "search_disease",
        &disease_id,
        &document,
    )
    .await;

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

/// Returns the prefixes of a string, one for each of its lengths.
///
/// The search views hold the prefixes of a name, so that a search matches a
/// name that is only partially typed.
fn ngrams(value: &str) -> Vec<String> {
    value
        .chars()
        .scan(String::new(), |accumulator, character| {
            accumulator.push(character);
            Some(accumulator.clone())
        })
        .collect()
}

/// Indexes search documents into an index.
async fn index_search_documents(
    client: &reqwest::Client,
    url: &str,
    namespace: &str,
    index: &str,
    documents: &[Value],
) {
    let mut bulk = String::new();
    for document in documents {
        let id = document["id"].as_str().expect("search document has an id");
        writeln!(
            bulk,
            r#"{{"index":{{"_index":"{namespace}_{index}","_id":"{id}"}}}}"#
        )
        .expect("write search document action");
        bulk.push_str(&serde_json::to_string(document).expect("serialize search document"));
        bulk.push('\n');
    }
    let response = client
        .post(format!("{url}/{namespace}_{index}/_bulk"))
        .header("Content-Type", "application/x-ndjson")
        .body(bulk)
        .send()
        .await
        .expect("index OpenSearch fixtures");
    assert!(
        response.status().is_success(),
        "index OpenSearch {index} fixtures failed: {}",
        response.text().await.unwrap_or_default()
    );
    // The bulk endpoint answers with a success status even when some of the
    // documents are rejected, so the response body has to be checked as well.
    // Otherwise the index would be incomplete and the tests would still pass.
    let result: Value = response
        .json()
        .await
        .expect("read the OpenSearch bulk response");
    let failures: Vec<&Value> = result["items"]
        .as_array()
        .expect("OpenSearch bulk items")
        .iter()
        .filter(|item| {
            item.as_object()
                .and_then(|operation| operation.values().next())
                .is_some_and(|result| result["status"].as_u64().is_none_or(|status| status >= 300))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "indexing {failures:?} of the OpenSearch {index} fixtures failed"
    );
}

/// Indexes a single search document into an index.
async fn index_search_document(
    client: &reqwest::Client,
    url: &str,
    namespace: &str,
    index: &str,
    id: &str,
    document: &Value,
) {
    let response = client
        .post(format!("{url}/{namespace}_{index}/_doc/{id}"))
        .json(document)
        .send()
        .await
        .expect("index OpenSearch fixture");
    assert!(
        response.status().is_success(),
        "index OpenSearch {index} fixture failed: {}",
        response.text().await.unwrap_or_default()
    );
}

/// Returns the GraphQL errors of a response.
pub fn graphql_errors(response: &Value) -> &[Value] {
    response["errors"]
        .as_array()
        .map_or(&[][..], |errors| errors.as_slice())
}

/// Asserts that the response carries no GraphQL errors.
///
/// The whole response is part of the message: a GraphQL error says what went
/// wrong, but not what the server answered.
pub fn assert_no_errors(response: &Value) {
    assert_eq!(
        graphql_errors(response),
        &[] as &[Value],
        "unexpected GraphQL errors: {response}"
    );
}
