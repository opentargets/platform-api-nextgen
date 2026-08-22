use async_graphql::{Context, Object, SimpleObject};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::instrument;

use crate::{datasource::opensearch::OpenSearch, query::paginate::Page};

const FACET_ENTITIES: &[&str] = &["disease", "target", "drug", "study", "variant"];

/// Returns the facet search indices for the given entity names.
fn facet_indices(entity_names: Option<&[String]>) -> Vec<String> {
    let entities = match entity_names {
        Some(names) => names
            .iter()
            .map(String::as_str)
            .filter(|n| FACET_ENTITIES.contains(n))
            .collect(),
        None => FACET_ENTITIES.to_vec(),
    };
    entities
        .iter()
        .map(|e| format!("facet_search_{e}"))
        .collect()
}

// ---- models ----

/// Facet search hit for a single category item.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FacetDoc {
    label: String,
    category: String,
    datasource_id: String,
    entity_ids: Vec<String>,
}

/// Facet search hit for a single category item.
#[derive(Debug, SimpleObject)]
struct SearchFacetResult {
    /// Facet identifier.
    id: String,
    /// Human-readable facet label.
    label: String,
    /// Facet category this item belongs to (e.g., target, disease).
    category: String,
    /// Identifier of the datasource contributing this facet.
    datasource_id: String,
    /// Optional list of underlying entity identifiers represented by this facet.
    entity_ids: Vec<String>,
    /// Highlighted text snippets showing why this facet matched the query.
    highlights: Vec<String>,
    /// Relevance score of the facet hit for the current query.
    score: f64,
}

#[derive(Debug, SimpleObject)]
/// Facet search category with result count.
pub struct SearchFacetsCategory {
    /// Facet category name.
    name: String,
    /// Total number of results in this category.
    total: u64,
}

/// Facet search results including hits and category counts.
#[derive(Debug, SimpleObject)]
pub struct SearchFacetsResults {
    /// Total number of facetable results for the current query.
    total: u64,
    /// List of facetable hits matching the query.
    hits: Vec<SearchFacetResult>,
    /// Facet categories with their result counts.
    categories: Vec<SearchFacetsCategory>,
}

// ---- query utilities ----

/// Builds the facet scoring.
///
/// `*` short-circuits to `match_all`. Otherwise a `bool.should` (OR) combines:
///   1. `multi_match`: fuzzy full-text over analyzed `label`/`datasourceId` for recall.
///   2. two `.keyword` `term` clauses, heavily boosted so exact hits rank on top.
fn build_strategy(query: &str) -> Value {
    if query == "*" {
        return json!({ "match_all": {} });
    }
    json!({
        "bool": {
            "should": [
                {
                    "multi_match": {
                        "query": query,          // the user's search string
                        "fuzziness": "AUTO",     // edit distance by term length
                        "prefix_length": 1,      // first char must match (cuts fuzzy noise)
                        "max_expansions": 50,    // cap fuzzy variants per field
                        "operator": "or",        // any token may match
                        "fields": ["label^100", "datasourceId^70"]
                    }
                },
                // exact, case-insensitive hits are heavily boosted
                { "term": { "label.keyword":        { "value": query, "case_insensitive": true, "boost": 10000 } } },
                { "term": { "datasourceId.keyword": { "value": query, "case_insensitive": true, "boost": 10000 } } }
            ]
        }
    })
}

/// Builds the request that gets the facet hits.
fn build_hits_body(query: &str, category: Option<&str>, page: Page) -> Value {
    let filter = match category {
        None | Some("" | "*") => json!({ "match_all": {} }),
        Some(c) => json!({ "term": { "category.keyword": c } }),
    };

    json!({
        "from": page.index * page.size,
        "size": page.size,
        "track_total_hits": true,           // compute the exact total, not a capped estimate
        "query": {
            "bool": {                       // combine scoring and filtering
                "must": [
                    build_strategy(query),
                    filter
                ]
            }
        },
        "highlight": {
            "type": "unified",
            "fields": { "label": {}, "datasourceId": {} }
        }
    })
}

/// Builds the request that gets the facet categories.
fn build_categories_body() -> Value {
    json!({
        "size": 0,   // aggregation only, skip documents
        "aggs": {
            "categories": {
                "terms": { "field": "category.keyword", "size": 1000 }
            }
        }
    })
}

fn parse_hits(json: &Value) -> (Vec<SearchFacetResult>, u64) {
    let total = json["hits"]["total"]["value"].as_u64().unwrap_or(0);
    let hits = json["hits"]["hits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| {
            let doc: FacetDoc = serde_json::from_value(h["_source"].clone()).ok()?;
            Some(SearchFacetResult {
                id: h["_id"].as_str()?.to_owned(), // OpenSearch's own _id, outside of _source
                label: doc.label,
                category: doc.category,
                datasource_id: doc.datasource_id,
                entity_ids: doc.entity_ids,
                highlights: h["highlight"]
                    .as_object()
                    .into_iter()
                    .flat_map(|m| m.values())
                    .filter_map(Value::as_array)
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect(),
                score: h["_score"].as_f64().unwrap_or(0.0),
            })
        })
        .collect();
    (hits, total)
}

fn parse_categories(json: &Value) -> Vec<SearchFacetsCategory> {
    json["aggregations"]["categories"]["buckets"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| SearchFacetsCategory {
            name: c["key"].as_str().unwrap_or_default().to_owned(),
            total: c["doc_count"].as_u64().unwrap_or(0),
        })
        .collect()
}

// ---- resolvers ----

#[derive(Default)]
pub struct FacetQuery;

#[Object]
impl FacetQuery {
    /// Faceted search over the `facet_search_*` indices.
    #[instrument(skip(self, ctx))]
    async fn facets(
        &self,
        ctx: &Context<'_>,
        query_string: String,
        entity_names: Option<Vec<String>>,
        category: Option<String>,
        #[graphql(default)] page: Page,
    ) -> Result<SearchFacetsResults, async_graphql::Error> {
        let os = ctx.data::<OpenSearch>()?;
        let indices = facet_indices(entity_names.as_deref());
        let idx: Vec<&str> = indices.iter().map(String::as_str).collect();

        // categories are always fetched
        let categories_body = build_categories_body();

        // empty query: return only category list
        if query_string.is_empty() {
            let cats = os
                .search(&idx, categories_body)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            return Ok(SearchFacetsResults {
                hits: vec![],
                total: 0,
                categories: parse_categories(&cats),
            });
        }

        // hits are fetched with the query
        let (hits_json, cats_json) = tokio::try_join!(
            os.search(
                &idx,
                build_hits_body(&query_string, category.as_deref(), page)
            ),
            os.search(&idx, categories_body),
        )
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        let (hits, total) = parse_hits(&hits_json);
        Ok(SearchFacetsResults {
            hits,
            total,
            categories: parse_categories(&cats_json),
        })
    }
}
