//! Cache implementation for Deepseek model responses
//! Provides caching to reduce API calls and improve latency

use crate::error::Result;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Cache entry with value and metadata
#[derive(Debug, Clone)]
struct CacheEntry<T> {
    /// Cached value
    value: T,
    /// When entry was created
    created_at: Instant,
    /// Number of times accessed
    hits: u64,
}

/// Configuration for cache behavior
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// How long entries remain valid
    pub ttl: Duration,
    /// Maximum cache size
    pub max_size: usize,
    /// Whether to track hit rates
    pub track_metrics: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(3600), // 1 hour
            max_size: 1000,
            track_metrics: true,
        }
    }
}

/// Cache for model responses
pub struct ModelCache<T> {
    entries: Arc<RwLock<HashMap<String, CacheEntry<T>>>>,
    config: CacheConfig,
}

impl<T: Clone + Send + Sync + 'static> ModelCache<T> {
    pub fn new(config: CacheConfig) -> Self {
        let cache = Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            config,
        };

        // Start cleanup task
        if config.ttl.as_secs() > 0 {
            cache.start_cleanup();
        }

        cache
    }

    /// Gets a value from cache if it exists and is valid
    pub async fn get(&self, key: &str) -> Option<T> {
        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(key) {
            if entry.created_at.elapsed() < self.config.ttl {
                return Some(entry.value.clone());
            }
        }
        None
    }

    /// Inserts a value into the cache
    pub async fn insert(&self, key: String, value: T) -> Result<()> {
        let mut entries = self.entries.write().await;

        // Check size limit
        if entries.len() >= self.config.max_size {
            // Remove oldest entry
            if let Some((oldest_key, _)) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.created_at)
            {
                let oldest_key = oldest_key.clone();
                entries.remove(&oldest_key);
            }
        }

        entries.insert(
            key,
            CacheEntry {
                value,
                created_at: Instant::now(),
                hits: 0,
            },
        );

        Ok(())
    }

    /// Removes expired entries
    async fn cleanup(&self) {
        let mut entries = self.entries.write().await;
        entries.retain(|_, entry| entry.created_at.elapsed() < self.config.ttl);
    }

    /// Starts background cleanup task
    fn start_cleanup(&self) {
        let entries = self.entries.clone();
        let ttl = self.config.ttl;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(ttl / 2);
            loop {
                interval.tick().await;
                let mut entries = entries.write().await;
                entries.retain(|_, entry| entry.created_at.elapsed() < ttl);
                debug!("Cache cleanup complete, remaining entries: {}", entries.len());
            }
        });
    }

    /// Gets cache statistics
    pub async fn stats(&self) -> CacheStats {
        let entries = self.entries.read().await;
        let total_hits: u64 = entries.values().map(|e| e.hits).sum();

        CacheStats {
            size: entries.len(),
            total_hits,
            hit_rate: if entries.is_empty() {
                0.0
            } else {
                total_hits as f64 / entries.len() as f64
            },
        }
    }
}

/// Cache performance statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of entries in cache
    pub size: usize,
    /// Total number of cache hits
    pub total_hits: u64,
    /// Average hits per entry
    pub hit_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_operations() {
        let config = CacheConfig {
            ttl: Duration::from_secs(1),
            max_size: 2,
            track_metrics: true,
        };
        let cache = ModelCache::new(config);

        // Test insertion
        cache.insert("key1".into(), "value1").await.unwrap();
        cache.insert("key2".into(), "value2").await.unwrap();

        // Test retrieval
        assert_eq!(cache.get("key1").await, Some("value1"));
        assert_eq!(cache.get("key2").await, Some("value2"));

        // Test size limit
        cache.insert("key3".into(), "value3").await.unwrap();
        assert_eq!(cache.get("key1").await, None); // Oldest entry removed

        // Test expiration
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(cache.get("key2").await, None);
        assert_eq!(cache.get("key3").await, None);
    }
} 