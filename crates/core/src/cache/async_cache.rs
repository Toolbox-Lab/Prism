use std::sync::Arc;

use serde::{de::DeserializeOwned, Serialize};

use crate::cache::disk::DiskCache;
use crate::cache::provider::CacheProvider;
use crate::error::GratResult;

#[derive(Clone)]
pub struct AsyncCache {
    inner: Arc<DiskCache>,
}

impl AsyncCache {
    pub fn new(inner: DiskCache) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub async fn get<V>(&self, key: &str) -> GratResult<Option<V>>
    where
        V: DeserializeOwned + Send + 'static,
    {
        let cache = Arc::clone(&self.inner);
        let key = key.to_owned();
        let handle = tokio::runtime::Handle::current();

        tokio::task::spawn_blocking(move || handle.block_on(cache.get::<V>(&key)))
            .await
            .expect("blocking cache task failed")
    }

    pub async fn put<V>(&self, key: &str, value: V) -> GratResult<()>
    where
        V: Serialize + Send + Sync + 'static,
    {
        let cache = Arc::clone(&self.inner);
        let key = key.to_owned();
        let handle = tokio::runtime::Handle::current();

        tokio::task::spawn_blocking(move || handle.block_on(cache.put(&key, &value)))
            .await
            .expect("blocking cache task failed")
    }
}