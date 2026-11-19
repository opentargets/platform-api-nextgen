mod common;

use common::{
    OTHER_TARGET_ID, OTHER_TARGET_SYMBOL, TARGET_ID, TARGET_NAME, TARGET_SYMBOL, environment,
    graphql_errors,
};
use serde_json::json;

#[tokio::test]
async fn search_returns_deterministic_target_hit_and_aggregations() {
    let environment = environment().await;

    let query = json!({
        "queryString": TARGET_SYMBOL,
        "entityNames": ["target"],
    });
    let response = environment
        .graphql_with_variables(
            r"
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
        ",
            &query,
        )
        .await;

    assert!(graphql_errors(&response).is_empty());
    assert_eq!(response["data"]["search"]["total"], 1);
    assert_eq!(response["data"]["search"]["hits"][0]["id"], TARGET_ID);
    assert_eq!(response["data"]["search"]["hits"][0]["name"], TARGET_NAME);
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
async fn search_resolves_target_object_from_clickhouse() {
    let environment = environment().await;

    let response = environment
        .graphql(
            r#"
            query {
              search(queryString: "BRCA2", entityNames: [target]) {
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
            "#,
        )
        .await;

    assert!(graphql_errors(&response).is_empty());
    let hit = &response["data"]["search"]["hits"][0];
    assert_eq!(hit["id"], TARGET_ID);
    assert_eq!(hit["object"]["__typename"], "Target");
    assert_eq!(hit["object"]["id"], TARGET_ID);
    assert_eq!(hit["object"]["approvedSymbol"], TARGET_SYMBOL);
    assert_eq!(hit["object"]["approvedName"], TARGET_NAME);
    assert_eq!(hit["object"]["biotype"], "protein_coding");
}

#[tokio::test]
async fn target_queries_return_deterministic_targets() {
    let environment = environment().await;

    let response = environment
        .graphql(
            r#"
            query {
              target(ensemblId: "ENSG00000139618") {
                id
                approvedSymbol
                approvedName
                biotype
              }
              targets(ensemblIds: ["ENSG00000141510", "ENSG00000139618"]) {
                count
                rows { id approvedSymbol }
              }
            }
            "#,
        )
        .await;

    assert!(graphql_errors(&response).is_empty());
    assert_eq!(response["data"]["target"]["id"], TARGET_ID);
    assert_eq!(response["data"]["target"]["approvedSymbol"], TARGET_SYMBOL);
    assert_eq!(response["data"]["target"]["approvedName"], TARGET_NAME);
    assert_eq!(response["data"]["target"]["biotype"], "protein_coding");
    assert_eq!(response["data"]["targets"]["count"], 2);
    assert_eq!(
        response["data"]["targets"]["rows"][0]["id"],
        OTHER_TARGET_ID
    );
    assert_eq!(
        response["data"]["targets"]["rows"][0]["approvedSymbol"],
        OTHER_TARGET_SYMBOL
    );
    assert_eq!(response["data"]["targets"]["rows"][1]["id"], TARGET_ID);
    assert_eq!(
        response["data"]["targets"]["rows"][1]["approvedSymbol"],
        TARGET_SYMBOL
    );
}

#[tokio::test]
async fn unknown_target_is_null() {
    let environment = environment().await;

    let response = environment
        .graphql(
            r#"
            query {
              target(ensemblId: "ENSG00000000000") { id }
            }
            "#,
        )
        .await;

    assert!(graphql_errors(&response).is_empty());
    assert!(response["data"]["target"].is_null());
}
