use async_graphql::{Context, Object, SimpleObject};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::instrument;

use crate::{datasource::opensearch::OpenSearch, query::paginate::Page};

const SEARCH_INDICES: &[&str] = &[
    "search_disease",
    "search_target",
    "search_drug",
    "search_study",
    "search_variant",
];

// ---- models ----

/// Represents a raw search document returned from OpenSearch.
#[derive(Debug, Deserialize)]
struct SearchDoc {
    id: String,
    name: String,
    entity: String,
    #[serde(default)]
    category: Vec<String>,
    description: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    prefixes: Vec<String>,
    #[serde(default)]
    ngrams: Vec<String>,
    #[serde(default = "default_multiplier")]
    multiplier: f64,
}

fn default_multiplier() -> f64 { 1.0 }

/// Full-text search hit describing a single entity and its relevance to the
/// query.
#[derive(Debug, SimpleObject)]
pub struct SearchResult {
    /// Entity identifier (e.g., Ensembl, EFO, ChEMBL, variant or study ID).
    pub id: String,
    /// Primary display name for the entity.
    pub name: String,
    /// Entity type (target, disease, drug, variant, study).
    pub entity: String,
    /// List of categories the hit belongs to.
    pub category: Vec<String>,
    /// Short description or summary of the entity.
    pub description: Option<String>,
    /// Additional keywords associated with the entity.
    pub keywords: Vec<String>,
    /// List of name prefixes used for prefix matching.
    pub prefixes: Vec<String>,
    /// List of n-grams derived from the name used for fuzzy matching.
    pub ngrams: Vec<String>,
    /// Highlighted text snippets showing where the query matched.
    pub highlights: Vec<String>,
    /// Score boosting multiplier applied to the hit during ranking.
    pub multiplier: f64,
    /// Relevance score returned from the search engine for this hit.
    pub score: f64,
    // TODO: object: Option<EntityUnionType>
    // Resolved entity corresponding to the search hit.
}

/// Search results including hits and facet aggregations.
#[derive(Debug, SimpleObject)]
pub struct SearchResults {
    /// Total number of results for the current query and entity filter.
    pub total: u64,
    /// Combined list of search hits across requested entities.
    pub hits: Vec<SearchResult>,
    // Facet aggregations by entity and category for the current query
    // TODO: aggregations: SearchResultAggregations
}

// ---- query utilities ----

/// Builds the OpenSearch request body for full-text entity search.
///
/// Combines three scoring strategies in a `bool.should` (with OR).
///   1. keyword: all query tokens must match one of the .raw id/keywords/name fields (analyzed via
///      token), heavily boosted so exact id/name hits come out on top.
///   2. `string`: fuzzy full-text over the analyzed recall fields (name, ngrams, term-expansions),
///      scaled by the doc's `multiplier`.
///   3. `exact`: one case-insensitive `term` per `.raw` field, each scaled by `multiplier`, giving
///      exact matches a large boost.
///
/// `entities` becomes a non-scoring `filter` (narrows results, leaves scores untouched). `page`
/// maps to offset/limit; `track_total_hits` forces an exact total; the bulky `terms*` expansion
/// fields are excluded from `_source` to keep responses small.
fn build_body(query: &str, entities: Option<&[String]>, page: Page) -> Value {
    let keyword = json!({
        "multi_match": {
            "query": query,           // the user's search string
            "analyzer": "token",      // tokenize the query with the index's `token` analyzer
            "type": "best_fields",    // score = the highest-scoring single field, not a sum
            "operator": "and",        // every token must be present inside a field
            "fields": [               // fields to match against, with relevance boosts (^N)
                "id.raw^1000",
                "keywords.raw^1000",
                "name.raw^1000"
            ]
        }
    });

    let string = json!({
        "function_score": {                       // we wrap to apply the multiplier
            "query": {
                "simple_query_string": {          // discards invalid portions instead of erroring
                    "query": query,               // the user's search string
                    "analyzer": "token",          // same tokenization as strategy 1
                    "minimum_should_match": "0",  // don't require any minimum term match
                    "default_operator": "AND",    // multi-term queries prefer all terms
                    "fields": [                   // analyzed to match against, with relevance boosts (^N)
                        "name^50",
                        "description^25",
                        "prefixes^20",
                        "terms5^15",
                        "terms25^10",
                        "terms^5",
                        "ngrams"
                    ]
                }
            },
            "field_value_factor": {               // multiply the query score by a field's value
                "field": "multiplier",            // the multiplier stored on the document
                "factor": 1.0,                    // no extra scaling
                "modifier": "none"                // no modifier
            }
        }
    });

    let exact_fields = [
        ("id.raw", 1000.0),
        ("keywords.raw", 1000.0),
        ("name.raw", 1000.0),
        ("prefixes.raw", 500.0),
        ("terms5.raw", 100.0),
        ("terms25.raw", 50.0),
        ("terms.raw", 25.0),
        ("ngrams.raw", 1.0),
    ];
    let exact: Vec<Value> = exact_fields
        .iter()
        .map(|&(f, boost)| {
            json!({
                "function_score": {                        // we wrap to apply the multiplier
                    "query": {
                        "term": {                          // search for an exact term in a field
                            f: {                           // field
                                "value": query,            // the user's search string
                                "case_insensitive": true,
                                "boost": boost
                            }
                        }
                    },
                    "field_value_factor": {                // same multiplier scaling as strategy 2
                        "field": "multiplier",
                        "factor": 1.0,
                        "modifier": "none"
                    }
                }
            })
        })
        .collect();

    // Assemble the OR set: the two standalone strategies plus the 8 exact clauses.
    let mut should = vec![keyword, string];
    should.extend(exact);

    // Entity filter
    let filter = entities.map_or_else(|| json!([]), |e| json!([{ "terms": { "entity.raw": e } }]));

    // Final request body: pagination, exact totals, the scored+filtered query, and source trimming
    // to drop the heavy expansion fields from each returned document.
    json!({
        "from": page.index * page.size,
        "size": page.size,
        "track_total_hits": true,         // compute the exact total, not a capped estimate
        "query": {
            "bool": {                     // combine scoring and filtering
                "must": {                 // scoring part (contributes to relevance)
                    "bool": {
                        "should": should  // the OR set of strategies built above
                    }
                },
                "filter": filter          // non-scoring narrowing by entity type
            }
        },
        "_source": {
            "excludes": [                 // omit these from returned documents (large, unused)
                "terms",
                "terms5",
                "terms25"
            ]
        }
    })
}

fn parse(json: &Value) -> SearchResults {
    let total = json["hits"]["total"]["value"].as_u64().unwrap_or(0);
    let hits = json["hits"]["hits"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|h| {
                    let doc: SearchDoc = serde_json::from_value(h["_source"].clone()).ok()?;
                    let score = h["_score"].as_f64().unwrap_or(0.0);
                    Some(SearchResult {
                        id: doc.id,
                        name: doc.name,
                        entity: doc.entity,
                        category: doc.category,
                        description: doc.description,
                        keywords: doc.keywords,
                        prefixes: doc.prefixes,
                        ngrams: doc.ngrams,
                        highlights: Vec::new(),
                        multiplier: doc.multiplier,
                        score,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    SearchResults { total, hits }
}

// ---- resolvers ----

#[derive(Default)]
pub struct SearchQuery;

#[Object]
impl SearchQuery {
    #[instrument(skip(self, ctx))]
    async fn search(
        &self,
        ctx: &Context<'_>,
        query_string: String,
        entity_names: Option<Vec<String>>,
        #[graphql(default)] page: Page,
    ) -> Result<SearchResults, async_graphql::Error> {
        if query_string.is_empty() {
            return Ok(SearchResults {
                hits: Vec::new(),
                total: 0,
            });
        }

        let os = ctx.data::<OpenSearch>()?;
        let body = build_body(&query_string, entity_names.as_deref(), page);
        let json = os
            .search(SEARCH_INDICES, body)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(parse(&json))
    }
}
