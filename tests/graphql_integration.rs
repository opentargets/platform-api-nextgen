mod common;

use common::{assert_no_errors, environment, frontend_queries, graphql_errors, sample_data};
use serde_json::{Value, json};

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

/// The query that searches the drug index and resolves the hit to the drug
/// stored in ClickHouse.
const SEARCH_DRUG_QUERY: &str = r"
query SearchDrug($queryString: String!) {
  search(queryString: $queryString, entityNames: [drug]) {
    total
    hits {
      id
      name
      entity
      object {
        __typename
        ... on Drug {
          id
          name
        }
      }
    }
  }
}
";

/// The query that searches the disease index and resolves the hit to the
/// disease stored in ClickHouse.
const SEARCH_DISEASE_QUERY: &str = r"
query SearchDisease($queryString: String!) {
  search(queryString: $queryString, entityNames: [disease]) {
    total
    hits {
      id
      name
      entity
      object {
        __typename
        ... on Disease {
          id
          name
        }
      }
    }
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
    let query = frontend_queries::query("TargetPage_TargetPage");

    let response = environment
        .graphql_with_variables(
            &frontend_queries::executable(query),
            &frontend_queries::variables(query),
        )
        .await;

    assert_no_errors(&response);
    let target = &response["data"]["target"];
    assert_eq!(target["id"], sample_data::first_target_id());
    assert_eq!(target["approvedSymbol"], sample_data::first_target_symbol());
    assert_eq!(target["approvedName"], sample_data::first_target_name());
    assert_ne!(
        target["proteinIds"].as_array().expect("proteinIds"),
        &[] as &[Value]
    );
    assert_ne!(
        target["dbXrefs"].as_array().expect("dbXrefs"),
        &[] as &[Value]
    );
}

#[tokio::test]
async fn targets_query_returns_the_requested_targets_in_order() {
    let environment = environment().await;

    let response = environment.graphql(TARGETS_QUERY).await;

    assert_no_errors(&response);
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

    assert_no_errors(&response);
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

    assert_no_errors(&response);
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

    assert_no_errors(&response);
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

    assert_no_errors(&response);
    // The field has to be there, and explicitly null: indexing the response
    // would also return null for a missing field, which would hide a server
    // that does not return the target at all.
    assert_eq!(
        response.pointer("/data/target"),
        Some(&Value::Null),
        "the target of an unknown id should be explicitly null"
    );
}

/// Every document the frontend sends to this API is executed against the
/// sample data.
///
/// The documents are the ones the frontend actually runs, copied verbatim from
/// `platform-webapp`, so a change in the API that breaks one of them fails
/// here.
#[tokio::test]
async fn the_frontend_queries_are_served() {
    let environment = environment().await;
    frontend_queries::check_manifest();

    for query in frontend_queries::QUERIES {
        let response = environment
            .graphql_with_variables(
                &frontend_queries::executable(query),
                &frontend_queries::variables(query),
            )
            .await;

        let errors = graphql_errors(&response);
        assert_eq!(
            errors,
            &[] as &[Value],
            "the frontend query {} (from {}) failed: {errors:?}",
            query.name,
            query.source
        );
        // The query has to have resolved the entity of the sample data. A
        // query that returns nothing would pass the check above. Not every
        // document selects the identifier, so only the entity itself is
        // checked.
        let path = query.root.path();
        assert!(
            response
                .pointer(&path)
                .is_some_and(|entity| !entity.is_null()),
            "the frontend query {} did not resolve the entity",
            query.name
        );
        // The fields the sample data has values for have to carry them.
        // Without this check a document would pass as long as it resolved the
        // entity, even if the API returned nothing for all of it.
        //
        // Not every field can be checked: the sample data has no values for
        // the chemical probes, the pathways and the safety liabilities of its
        // targets, so an empty list is the correct answer for those.
        for field in query.expected {
            let value = response
                .pointer(&format!("{path}/{field}"))
                .unwrap_or_else(|| panic!("the frontend query {} has no {field}", query.name));
            assert!(
                !is_empty(value),
                "the frontend query {} returned nothing for {field}",
                query.name
            );
        }
    }
}

/// Returns whether the value carries nothing.
///
/// A field the API has no value for is null, a list is empty, and an object
/// with no fields is empty as well.
fn is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Array(values) => values.is_empty(),
        Value::Object(values) => values.is_empty(),
        _ => false,
    }
}

/// The drug page of the frontend returns the drug of the sample data.
#[tokio::test]
async fn drug_page_query_returns_the_drug_from_the_sample_data() {
    let environment = environment().await;
    let query = frontend_queries::query("DrugPage_DrugPage");

    let response = environment
        .graphql_with_variables(
            &frontend_queries::executable(query),
            &frontend_queries::variables(query),
        )
        .await;

    assert_no_errors(&response);
    let drug = &response["data"]["drug"];
    assert_eq!(drug["id"], sample_data::first_drug_id());
    assert_eq!(drug["name"], sample_data::first_drug_name());
    assert_ne!(
        drug["crossReferences"].as_array().expect("crossReferences"),
        &[] as &[Value]
    );
}

/// The disease page of the frontend returns the disease of the sample data.
#[tokio::test]
async fn disease_page_query_returns_the_disease_from_the_sample_data() {
    let environment = environment().await;
    let query = frontend_queries::query("DiseasePage_DiseasePage");

    let response = environment
        .graphql_with_variables(
            &frontend_queries::executable(query),
            &frontend_queries::variables(query),
        )
        .await;

    assert_no_errors(&response);
    let disease = &response["data"]["disease"];
    assert_eq!(disease["id"], sample_data::first_disease_id());
    assert_eq!(disease["name"], sample_data::first_disease_name());
    assert_ne!(
        disease["dbXRefs"].as_array().expect("dbXRefs"),
        &[] as &[Value]
    );
}

/// A search for a drug returns a hit that resolves to the drug stored in
/// ClickHouse.
#[tokio::test]
async fn search_resolves_the_drug_object_from_clickhouse() {
    let environment = environment().await;

    let response = environment
        .graphql_with_variables(
            SEARCH_DRUG_QUERY,
            &json!({
                "queryString": sample_data::first_drug_name(),
                "entityNames": ["drug"],
            }),
        )
        .await;

    assert_no_errors(&response);
    let hit = &response["data"]["search"]["hits"][0];
    assert_eq!(hit["id"], sample_data::first_drug_id());
    assert_eq!(hit["entity"], "drug");
    assert_eq!(hit["object"]["__typename"], "Drug");
    assert_eq!(hit["object"]["id"], sample_data::first_drug_id());
    assert_eq!(hit["object"]["name"], sample_data::first_drug_name());
}

/// A search for a disease returns a hit that resolves to the disease stored in
/// ClickHouse.
#[tokio::test]
async fn search_resolves_the_disease_object_from_clickhouse() {
    let environment = environment().await;

    let response = environment
        .graphql_with_variables(
            SEARCH_DISEASE_QUERY,
            &json!({
                "queryString": sample_data::first_disease_name(),
                "entityNames": ["disease"],
            }),
        )
        .await;

    assert_no_errors(&response);
    let hit = &response["data"]["search"]["hits"][0];
    assert_eq!(hit["id"], sample_data::first_disease_id());
    assert_eq!(hit["entity"], "disease");
    assert_eq!(hit["object"]["__typename"], "Disease");
    assert_eq!(hit["object"]["id"], sample_data::first_disease_id());
    assert_eq!(hit["object"]["name"], sample_data::first_disease_name());
}
