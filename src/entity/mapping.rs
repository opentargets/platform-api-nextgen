use async_graphql::{Context, Object, SimpleObject};
use serde_json::{Value, json};
use tracing::instrument;

use crate::{
    datasource::opensearch::OpenSearch,
    entity::search::{
        EntityType, SEARCH_INDICES, SearchResult, SearchResultAggs, entity_aggs, parse_aggs,
        parse_hits, search_indices,
    },
};

/// Maximum number of hits fetched for a mapping request.
const MAX_MAPPING_HITS: u32 = 10_000;

// ---- models ----

/// Mapping result for a single input term.
#[derive(Debug, SimpleObject)]
pub struct MappingResult {
    /// Input term submitted for mapping.
    term: String,
    /// Search hits that the term maps to, if any.
    hits: Option<Vec<SearchResult>>,
}

/// Mapping results for multiple terms with total hit count and aggregations.
#[derive(Debug, SimpleObject)]
pub struct MappingResults {
    /// Per-term mapping results.
    mappings: Vec<MappingResult>,
    /// Facet aggregations over mapped entities and categories.
    aggregations: Option<SearchResultAggs>,
    /// Total number of mapped hits across all terms.
    total: u64,
}

// ---- query utilities ----

/// Builds the exact match of the terms against the `keywords.raw` field.
fn build_terms_query(terms: &[String]) -> Value { json!({ "terms": { "keywords.raw": terms } }) }

/// Builds the request that gets the mapping hits.
fn build_hits_body(terms: &[String]) -> Value {
    json!({
        "from": 0,
        "size": MAX_MAPPING_HITS,
        "track_total_hits": true,         // compute the exact total, not a capped estimate
        "query": {
            "bool": {
                "filter": [build_terms_query(terms)]  // exact matching only, no scoring needed
            }
        },
        "highlight": {
            "type": "plain",
            "pre_tags": [""],             // no markup, so a highlight is the bare matched keyword
            "post_tags": [""],
            "fields": {
                "keywords.raw": {}
            }
        }
    })
}

/// Builds the request that gets the mapping aggregations.
fn build_aggs_body(terms: &[String]) -> Value {
    json!({
        "size": 0,
        "query": build_terms_query(terms),
        "aggs": entity_aggs()
    })
}

// ---- resolvers ----

#[derive(Default)]
pub struct MappingQuery;

#[Object]
impl MappingQuery {
    #[instrument(skip(self, ctx))]
    /// Map terms to entities (targets, diseases, drugs, variants or studies) by exact keyword
    /// match.
    async fn map_ids(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "List of query terms to map.")] query_terms: Vec<String>,
        #[graphql(desc = "List of entity names to search for (target, disease, drug, etc.).")]
        entity_names: Option<Vec<EntityType>>,
    ) -> Result<MappingResults, async_graphql::Error> {
        let terms: Vec<String> = query_terms.into_iter().filter(|t| !t.is_empty()).collect();
        if terms.is_empty() {
            return Ok(MappingResults {
                mappings: Vec::new(),
                total: 0,
                aggregations: None,
            });
        }

        let os = ctx.data::<OpenSearch>()?;
        let mapping_indices = search_indices(entity_names.as_deref());
        let hits_body = build_hits_body(&terms);
        let aggs_body = build_aggs_body(&terms);

        let (hits_json, aggs_json) = tokio::try_join!(
            os.search(&mapping_indices, hits_body),
            os.search(SEARCH_INDICES, aggs_body),
        )
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        let results = parse_hits(&hits_json);
        let mappings = terms
            .into_iter()
            .map(|term| {
                let needle = term.to_lowercase();
                let hits = results
                    .hits
                    .iter()
                    .filter(|h| h.highlights.contains(&needle))
                    .cloned()
                    .collect();
                MappingResult {
                    term,
                    hits: Some(hits),
                }
            })
            .collect();

        Ok(MappingResults {
            mappings,
            aggregations: parse_aggs(&aggs_json),
            total: results.total,
        })
    }
}
