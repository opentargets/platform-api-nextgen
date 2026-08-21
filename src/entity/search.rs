use std::collections::HashSet;

use async_graphql::{ComplexObject, Context, Object, SimpleObject, Union, dataloader::DataLoader};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::instrument;

use crate::{
    datasource::opensearch::OpenSearch,
    entity::{
        disease::{Disease, DiseaseLoader},
        study::{Study, StudyLoader},
    },
    query::paginate::Page,
};

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

/// Union of core Platform entities (target, disease, drug, variant, study).
#[derive(Union)]
pub enum EntityObject {
    Disease(Disease),
    Study(Study),
}

/// Full-text search hit describing a single entity and its relevance to the
/// query.
#[derive(Debug, SimpleObject)]
#[graphql(complex)]
pub struct SearchResult {
    /// Entity identifier (e.g., Ensembl, EFO, ChEMBL, variant or study ID).
    id: String,
    /// Primary display name for the entity.
    name: String,
    /// Entity type (target, disease, drug, variant, study).
    entity: String,
    /// List of categories the hit belongs to.
    category: Vec<String>,
    /// Short description or summary of the entity.
    description: Option<String>,
    /// Additional keywords associated with the entity.
    keywords: Vec<String>,
    /// List of name prefixes used for prefix matching.
    prefixes: Vec<String>,
    /// List of n-grams derived from the name used for fuzzy matching.
    ngrams: Vec<String>,
    /// Highlighted text snippets showing where the query matched.
    highlights: Vec<String>,
    /// Score boosting multiplier applied to the hit during ranking.
    multiplier: f64,
    /// Relevance score returned from the search engine for this hit.
    score: f64,
}

/// Search result aggregation category with result count.
#[derive(Debug, SimpleObject)]
pub struct SearchResultAggCategory {
    /// Category name (e.g., target, disease, drug).
    name: String,
    /// Total number of search results in this category.
    total: u64,
}

/// Search result aggregation by entity type with category breakdown.
#[derive(Debug, SimpleObject)]
pub struct SearchResultAggEntity {
    /// Entity type name (e.g., target, disease, drug, variant, study).
    name: String,
    /// Total number of search results in this entity type.
    total: u64,
    /// List of category aggregations within this entity type.
    categories: Vec<SearchResultAggCategory>,
}

/// Search result aggregations grouped by entity type.
#[derive(Debug, SimpleObject)]
pub struct SearchResultAggs {
    /// Total number of search results across all entities.
    total: u64,
    /// List of entity type aggregations with category breakdowns.
    entities: Vec<SearchResultAggEntity>,
}

/// Search results including hits and facet aggregations.
#[derive(Debug, SimpleObject)]
pub struct SearchResults {
    /// Total number of results for the current query and entity filter.
    total: u64,
    /// Combined list of search hits across requested entities.
    hits: Vec<SearchResult>,
    // Facet aggregations by entity and category for the current query.
    aggregations: Option<SearchResultAggs>,
}

// ---- query utilities ----

/// Builds the search strategy.
///
/// Combines three scoring strategies in a `bool.should` (with OR).
///   1. keyword: all query tokens must match one of the .raw id/keywords/name fields (analyzed via
///      token), heavily boosted so exact id/name hits come out on top.
///   2. `string`: fuzzy full-text over the analyzed recall fields (name, ngrams, term-expansions),
///      scaled by the doc's `multiplier`.
///   3. `exact`: one case-insensitive `term` per `.raw` field, each scaled by `multiplier`, giving
///      exact matches a large boost.
fn build_search_strategy(query: &str) -> Vec<Value> {
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
    should
}

/// Builds the OpenSearch request that gets the search hits.
fn build_hits_body(query: &str, entities: Option<&[String]>, page: Page) -> Value {
    let filter = entities.map_or_else(|| json!([]), |e| json!([{ "terms": { "entity.raw": e } }]));

    json!({
        "from": page.index * page.size,
        "size": page.size,
        "track_total_hits": true,         // compute the exact total, not a capped estimate
        "query": {
            "bool": {                     // combine scoring and filtering
                "must": {                 // scoring part (contributes to relevance)
                    "bool": {
                        "should": build_search_strategy(query)
                    }
                },
                "filter": filter          // non-scoring narrowing by entity type
            }
        },
        "highlight": {
            "type": "fvh",
            "fields": {
                "id": {},
                "keywords": {},
                "name": {},
                "description": {},
                "prefixes": {},
                "terms": {},
                "terms5": {},
                "terms25": {},
                "ngrams": {}
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

fn parse_hits(json: &Value) -> SearchResults {
    let total = json["hits"]["total"]["value"].as_u64().unwrap_or(0);
    let hits = json["hits"]["hits"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|h| {
                    let doc: SearchDoc = serde_json::from_value(h["_source"].clone()).ok()?;
                    let score = h["_score"].as_f64().unwrap_or(0.0);
                    let mut seen = HashSet::new();
                    let highlights: Vec<String> = h["highlight"]
                        .as_object()
                        .map(|m| {
                            m.values()
                                .filter_map(Value::as_array)
                                .flatten()
                                .filter_map(Value::as_str)
                                .filter(|s| seen.insert(s.to_string()))
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(SearchResult {
                        id: doc.id,
                        name: doc.name,
                        entity: doc.entity,
                        category: doc.category,
                        description: doc.description,
                        keywords: doc.keywords,
                        prefixes: doc.prefixes,
                        ngrams: doc.ngrams,
                        highlights,
                        multiplier: doc.multiplier,
                        score,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    SearchResults {
        total,
        hits,
        aggregations: None,
    }
}

/// Builds the aggregations body for the search query.
fn build_aggs_body(query: &str) -> Value {
    json!({
        "size": 0,
        "query": {
            "bool": {
                "should": build_search_strategy(query)
            }
        },
        "aggs": {
            "entities": {
                "terms": {
                    "field": "entity.raw",
                    "size": 1000
                },
                "aggs": {
                    "categories": {
                        "terms": {
                            "field": "category.raw",
                            "size": 1000
                        }
                    }
                }
            },
            "total": {
                "cardinality": {
                    "field": "id.raw"
                }
            }
        }
    })
}

fn parse_aggs(json: &Value) -> Option<SearchResultAggs> {
    let aggs = json.get("aggregations")?;
    let entities = aggs["entities"]["buckets"]
        .as_array()
        .map(|buckets| {
            buckets
                .iter()
                .map(|b| {
                    let categories = b["categories"]["buckets"]
                        .as_array()
                        .map(|cats| {
                            cats.iter()
                                .map(|c| SearchResultAggCategory {
                                    name: c["key"].as_str().unwrap_or_default().to_owned(),
                                    total: c["doc_count"].as_u64().unwrap_or(0),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    SearchResultAggEntity {
                        name: b["key"].as_str().unwrap_or_default().to_owned(),
                        total: b["doc_count"].as_u64().unwrap_or(0),
                        categories,
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let total = aggs["total"]["value"].as_u64().unwrap_or(0);
    Some(SearchResultAggs { total, entities })
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
                aggregations: None,
            });
        }

        let os = ctx.data::<OpenSearch>()?;
        let hits_body = build_hits_body(&query_string, entity_names.as_deref(), page);
        let aggs_body = build_aggs_body(&query_string);

        let (hits_json, aggs_json) = tokio::try_join!(
            os.search(SEARCH_INDICES, hits_body),
            os.search(SEARCH_INDICES, aggs_body),
        )
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        let mut results = parse_hits(&hits_json);
        results.aggregations = parse_aggs(&aggs_json);
        Ok(results)
    }
}

#[ComplexObject]
impl SearchResult {
    /// Resolved entity corresponding to the search hit.
    async fn object(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<EntityObject>, async_graphql::Error> {
        match self.entity.as_str() {
            "disease" => {
                let loader = ctx.data::<DataLoader<DiseaseLoader>>()?;
                Ok(loader
                    .load_one(self.id.clone())
                    .await?
                    .map(EntityObject::Disease))
            }
            "study" => {
                let loader = ctx.data::<DataLoader<StudyLoader>>()?;
                Ok(loader
                    .load_one(self.id.clone())
                    .await?
                    .map(EntityObject::Study))
            }
            _ => Ok(None),
        }
    }
}
