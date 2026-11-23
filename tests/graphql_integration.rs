mod common;

use common::{environment, graphql_errors, sample_data};
use serde_json::{Value, json};

/// The query the frontend runs to render a target page.
///
/// Taken verbatim from `platform-webapp`, in
/// `apps/platform/src/pages/TargetPage/TargetPage.gql`.
const TARGET_PAGE_QUERY: &str = r"
query TargetPageQuery($ensgId: String!) {
  target(ensemblId: $ensgId) {
    id
    approvedSymbol
    approvedName
    proteinIds {
      id
      source
    }
    dbXrefs {
      id
      source
    }
  }
}
";

/// The query that fetches several targets at once.
const TARGETS_QUERY: &str = r#"
query {
  targets(ensemblIds: ["ENSG00000000005", "ENSG00000000003"]) {
    count
    rows { id approvedSymbol approvedName biotype }
  }
}
"#;

/// The query that searches the target index and returns the hits with their
/// aggregations.
const SEARCH_QUERY: &str = r"
query SearchTarget($queryString: String!, $entityNames: [EntityType!]) {
  search(queryString: $queryString, entityNames: $entityNames) {
    total
    hits { id name entity highlights }
    aggregations {
      total
      entities { name total categories { name total } }
    }
  }
}
";

/// The query that searches the target index and resolves each hit to the
/// target stored in ClickHouse.
const SEARCH_WITH_OBJECT_QUERY: &str = r"
query SearchTarget($queryString: String!) {
  search(queryString: $queryString, entityNames: [target]) {
    hits {
      id
      object {
        __typename
        ... on Target {
          id
          approvedSymbol
          approvedName
          biotype
        }
      }
    }
  }
}
";

/// The query that searches the target index and returns the hits only.
const SEARCH_HITS_QUERY: &str = r"
query SearchTarget($queryString: String!) {
  search(queryString: $queryString, entityNames: [target]) {
    total
    hits { id name entity }
  }
}
";

/// The query that fetches a target that does not exist.
const UNKNOWN_TARGET_QUERY: &str = r#"
query {
  target(ensemblId: "ENSG00000000000") { id }
}
"#;

#[tokio::test]
async fn target_page_query_returns_the_target_from_the_sample_data() {
    let environment = environment().await;

    let response = environment
        .graphql_with_variables(
            TARGET_PAGE_QUERY,
            &json!({ "ensgId": sample_data::first_target_id() }),
        )
        .await;

    assert!(graphql_errors(&response).is_empty());
    let target = &response["data"]["target"];
    assert_eq!(target["id"], sample_data::first_target_id());
    assert_eq!(target["approvedSymbol"], sample_data::first_target_symbol());
    assert_eq!(target["approvedName"], sample_data::first_target_name());
    assert!(
        !target["proteinIds"]
            .as_array()
            .expect("proteinIds")
            .is_empty()
    );
    assert!(!target["dbXrefs"].as_array().expect("dbXrefs").is_empty());
}

#[tokio::test]
async fn targets_query_returns_the_requested_targets_in_order() {
    let environment = environment().await;

    let response = environment.graphql(TARGETS_QUERY).await;

    assert!(graphql_errors(&response).is_empty());
    let rows = response["data"]["targets"]["rows"]
        .as_array()
        .expect("targets rows");
    assert_eq!(response["data"]["targets"]["count"], 2);
    assert_eq!(rows[0]["id"], sample_data::second_target_id());
    assert_eq!(
        rows[0]["approvedSymbol"],
        sample_data::second_target_symbol()
    );
    assert_eq!(rows[0]["approvedName"], sample_data::second_target_name());
    assert_eq!(rows[1]["id"], sample_data::first_target_id());
    assert_eq!(
        rows[1]["approvedSymbol"],
        sample_data::first_target_symbol()
    );
}

#[tokio::test]
async fn search_returns_a_deterministic_target_hit_and_aggregations() {
    let environment = environment().await;

    let response = environment
        .graphql_with_variables(
            SEARCH_QUERY,
            &json!({
                "queryString": sample_data::first_target_symbol(),
                "entityNames": ["target"],
            }),
        )
        .await;

    assert!(graphql_errors(&response).is_empty());
    assert_eq!(response["data"]["search"]["total"], 1);
    assert_eq!(
        response["data"]["search"]["hits"][0]["id"],
        sample_data::first_target_id()
    );
    assert_eq!(
        response["data"]["search"]["hits"][0]["name"],
        sample_data::first_target_name()
    );
    assert_eq!(response["data"]["search"]["hits"][0]["entity"], "target");
    assert_eq!(response["data"]["search"]["aggregations"]["total"], 1);
    assert_eq!(
        response["data"]["search"]["aggregations"]["entities"][0]["name"],
        "target"
    );
    assert_eq!(
        response["data"]["search"]["aggregations"]["entities"][0]["total"],
        1
    );
}

#[tokio::test]
async fn search_resolves_the_target_object_from_clickhouse() {
    let environment = environment().await;

    let response = environment
        .graphql_with_variables(
            SEARCH_WITH_OBJECT_QUERY,
            &json!({ "queryString": sample_data::first_target_symbol() }),
        )
        .await;

    assert!(graphql_errors(&response).is_empty());
    let hit = &response["data"]["search"]["hits"][0];
    assert_eq!(hit["id"], sample_data::first_target_id());
    assert_eq!(hit["object"]["__typename"], "Target");
    assert_eq!(hit["object"]["id"], sample_data::first_target_id());
    assert_eq!(
        hit["object"]["approvedSymbol"],
        sample_data::first_target_symbol()
    );
    assert_eq!(
        hit["object"]["approvedName"],
        sample_data::first_target_name()
    );
    assert_eq!(hit["object"]["biotype"], "protein_coding");
}

#[tokio::test]
async fn search_returns_the_documents_of_the_sample_data() {
    let environment = environment().await;

    let response = environment
        .graphql_with_variables(
            SEARCH_HITS_QUERY,
            &json!({ "queryString": sample_data::first_search_target_name() }),
        )
        .await;

    assert!(graphql_errors(&response).is_empty());
    assert_eq!(response["data"]["search"]["total"], 1);
    assert_eq!(
        response["data"]["search"]["hits"][0]["id"],
        sample_data::first_search_target_id()
    );
    assert_eq!(
        response["data"]["search"]["hits"][0]["name"],
        sample_data::first_search_target_name()
    );
    assert_eq!(response["data"]["search"]["hits"][0]["entity"], "target");
}

#[tokio::test]
async fn unknown_target_is_null() {
    let environment = environment().await;

    let response = environment.graphql(UNKNOWN_TARGET_QUERY).await;

    assert!(graphql_errors(&response).is_empty());
    // The field has to be there, and explicitly null: indexing the response
    // would also return null for a missing field, which would hide a server
    // that does not return the target at all.
    assert_eq!(
        response.pointer("/data/target"),
        Some(&Value::Null),
        "the target of an unknown id should be explicitly null"
    );
}
