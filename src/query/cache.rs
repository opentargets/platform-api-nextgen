use std::{collections::HashMap, hash::Hash};

use async_graphql::Error;
use moka::future::Cache;

use crate::config::CACHE_ENTITY_CAPACITY;

#[must_use]
pub fn entity_cache<V: Clone + Send + Sync + 'static>() -> Cache<String, Option<V>> {
    Cache::builder().max_capacity(CACHE_ENTITY_CAPACITY).build()
}

pub trait CachedLoader {
    type Key: Clone + Eq + Hash + Send + Sync + 'static;
    type Value: Clone + Send + Sync + 'static;

    fn cache(&self) -> &Cache<Self::Key, Option<Self::Value>>;
    fn key_of(v: &Self::Value) -> Self::Key;

    #[allow(async_fn_in_trait)]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, Error>;

    #[allow(async_fn_in_trait)]
    #[tracing::instrument(
        skip_all,
        level = "debug",
        fields(
            keys = keys.len(),
            hits = tracing::field::Empty,
            misses = tracing::field::Empty,
        )
    )]
    async fn load_cached(
        &self,
        keys: &[Self::Key],
    ) -> Result<HashMap<Self::Key, Self::Value>, Error> {
        let mut result = HashMap::new();
        let mut misses = Vec::new();

        // first, we fill the result with cached values, and collect misses
        for key in keys {
            match self.cache().get(key).await {
                Some(Some(value)) => {
                    result.insert(key.clone(), value.clone());
                }
                Some(None) => {}
                None => misses.push(key.clone()),
            }
        }

        // log cache stats
        let span = tracing::Span::current();
        span.record("hits", result.len());
        span.record("misses", misses.len());

        // if there are no misses, we're done
        if misses.is_empty() {
            return Ok(result);
        }

        // otherwise, we fetch the missing values
        let mut index: HashMap<Self::Key, Self::Value> = self
            .fetch(&misses)
            .await?
            .into_iter()
            .map(|v| (Self::key_of(&v), v))
            .collect();
        for key in &misses {
            match index.remove(key) {
                Some(v) => {
                    self.cache().insert(key.clone(), Some(v.clone())).await;
                    result.insert(key.clone(), v);
                }
                None => {
                    self.cache().insert(key.clone(), None).await;
                }
            }
        }

        Ok(result)
    }
}
