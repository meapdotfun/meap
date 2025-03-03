//! Caching system for Deepseek model responses
//! Provides LRU and TTL-based caching to reduce redundant model calls

use crate::error::{DeepseekError, Result};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{debug, trace};

/// Configuration for the cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Whether caching is enabled
    pub enabled: bool,
    /// Maximum size of the cache
    pub max_entries: usize,
    /// Time-to-live for cached entries
    pub ttl: Duration,
    /// Whether to enable semantic caching
    pub semantic_caching: bool,
    /// Similarity threshold for semantic caching (0.0-1.0)
    pub similarity_threshold: f32,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 1000,
            ttl: Duration::from_secs(3600), // 1 hour
            semantic_caching: false,
            similarity_threshold: 0.95,
        }
    }
}

/// Key for cache lookups
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// Model ID
    pub model: String,
    /// Prompt hash
    pub prompt_hash: u64,
    /// System message hash (if present)
    pub system_hash: Option<u64>,
    /// Temperature used
    pub temperature: u32,
    /// Max tokens generated
    pub max_tokens: u32,
}

impl CacheKey {
    /// Create a new cache key from components
    pub fn new(
        model: &str,
        prompt: &str,
        system: Option<&str>,
        temperature: f32,
        max_tokens: u32,
    ) -> Self {
        // Hash the prompt and system message
        let mut prompt_hasher = DefaultHasher::new();
        prompt.hash(&mut prompt_hasher);
        let prompt_hash = prompt_hasher.finish();

        let system_hash = system.map(|s| {
            let mut system_hasher = DefaultHasher::new();
            s.hash(&mut system_hasher);
            system_hasher.finish()
        });

        // Quantize temperature to reduce cache misses (0.01 precision)
        let temperature_int = (temperature * 100.0) as u32;

        Self {
            model: model.to_string(),
            prompt_hash,
            system_hash,
            temperature: temperature_int,
            max_tokens,
        }
    }
}

/// Cached entry with metadata
#[derive(Debug, Clone)]
struct CacheEntry {
    /// Cached response
    pub response: String,
    /// When the entry was created
    pub created_at: Instant,
    /// Original prompt (for semantic matching)
    pub original_prompt: Option<String>,
}

/// Cache manager for model responses
pub struct ResponseCache {
    /// Configuration
    config: CacheConfig,
    /// LRU Cache protected by RwLock
    cache: Arc<RwLock<LruCache<CacheKey, CacheEntry>>>,
}

impl ResponseCache {
    /// Create a new cache with the given configuration
    pub fn new(config: CacheConfig) -> Self {
        let cache = LruCache::new(config.max_entries);

        Self {
            config,
            cache: Arc::new(RwLock::new(cache)),
        }
    }

    /// Store a response in the cache
    pub async fn store(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        temperature: f32,
        max_tokens: u32,
        response: String,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let key = CacheKey::new(model, prompt, system, temperature, max_tokens);
        
        let entry = CacheEntry {
            response,
            created_at: Instant::now(),
            original_prompt: if self.config.semantic_caching {
                Some(prompt.to_string())
            } else {
                None
            },
        };

        let mut cache = self.cache.write().await;
        cache.put(key, entry);
        
        debug!("Stored response in cache for model {}", model);
        Ok(())
    }

    /// Look up a response in the cache
    pub async fn lookup(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        temperature: f32,
        max_tokens: u32,
    ) -> Result<Option<String>> {
        if !self.config.enabled {
            return Ok(None);
        }

        let key = CacheKey::new(model, prompt, system, temperature, max_tokens);
        
        let mut cache = self.cache.write().await;
        
        // First try exact match
        if let Some(entry) = cache.get(&key) {
            // Check if entry is expired
            if entry.created_at.elapsed() > self.config.ttl {
                cache.pop(&key);
                return Ok(None);
            }
            
            debug!("Cache hit for model {}", model);
            return Ok(Some(entry.response.clone()));
        }
        
        // If semantic caching is enabled, try to find similar prompts
        if self.config.semantic_caching {
            trace!("Exact cache miss, attempting semantic match for model {}", model);
            
            // In a real implementation, we would use embeddings to find similar prompts
            // For now, we'll use a simple length comparison as a placeholder
            let prompt_len = prompt.len();
            let threshold = self.config.similarity_threshold;
            
            for (cached_key, entry) in cache.iter() {
                if cached_key.model != model {
                    continue;
                }
                
                // Check TTL
                if entry.created_at.elapsed() > self.config.ttl {
                    continue;
                }
                
                // Check if we have the original prompt (needed for semantic matching)
                if let Some(ref original) = entry.original_prompt {
                    // Placeholder for semantic matching - in real implementation, 
                    // this would compute cosine similarity between embeddings
                    let similarity = if (original.len() as f32 / prompt_len as f32).abs() > 0.8 {
                        // Simple placeholder for similar length
                        0.85
                    } else {
                        0.0
                    };
                    
                    if similarity > threshold {
                        debug!("Semantic cache hit with similarity {:.2} for model {}", similarity, model);
                        return Ok(Some(entry.response.clone()));
                    }
                }
            }
        }
        
        debug!("Cache miss for model {}", model);
        Ok(None)
    }

    /// Get the current cache stats
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        
        let mut expired_count = 0;
        let now = Instant::now();
        
        for entry in cache.iter() {
            if entry.1.created_at.elapsed() > self.config.ttl {
                expired_count += 1;
            }
        }
        
        CacheStats {
            total_entries: cache.len(),
            max_capacity: cache.cap(),
            expired_entries: expired_count,
            hit_count: 0, // We don't track this yet
            miss_count: 0, // We don't track this yet
        }
    }

    /// Clear all cached entries
    pub async fn clear(&self) -> Result<usize> {
        let mut cache = self.cache.write().await;
        let count = cache.len();
        cache.clear();
        Ok(count)
    }

    /// Clear expired entries
    pub async fn clear_expired(&self) -> Result<usize> {
        let mut cache = self.cache.write().await;
        let before_count = cache.len();
        
        // We need to collect keys first to avoid mutable borrow issues
        let mut expired_keys = Vec::new();
        
        for (key, entry) in cache.iter() {
            if entry.created_at.elapsed() > self.config.ttl {
                expired_keys.push(key.clone());
            }
        }
        
        for key in expired_keys {
            cache.pop(&key);
        }
        
        let removed = before_count - cache.len();
        Ok(removed)
    }
}

/// Statistics about the cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Current number of entries
    pub total_entries: usize,
    /// Maximum capacity
    pub max_capacity: usize,
    /// Number of expired entries
    pub expired_entries: usize,
    /// Cache hit count
    pub hit_count: usize,
    /// Cache miss count
    pub miss_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_cache_store_lookup() {
        let config = CacheConfig {
            enabled: true,
            max_entries: 10,
            ttl: Duration::from_secs(3600),
            semantic_caching: false,
            similarity_threshold: 0.0,
        };
        
        let cache = ResponseCache::new(config);
        
        // Store a response
        cache.store(
            "deepseek-coder",
            "What is Rust?",
            Some("Be helpful"),
            0.7,
            100,
            "Rust is a systems programming language.".to_string(),
        ).await.unwrap();
        
        // Lookup with same parameters
        let response = cache.lookup(
            "deepseek-coder",
            "What is Rust?",
            Some("Be helpful"),
            0.7,
            100,
        ).await.unwrap();
        
        assert_eq!(response, Some("Rust is a systems programming language.".to_string()));
        
        // Lookup with different parameters
        let response = cache.lookup(
            "deepseek-coder",
            "What is Rust?",
            Some("Different system prompt"),
            0.7,
            100,
        ).await.unwrap();
        
        assert_eq!(response, None);
    }
    
    #[tokio::test]
    async fn test_cache_expiry() {
        let config = CacheConfig {
            enabled: true,
            max_entries: 10,
            ttl: Duration::from_millis(100), // Very short TTL for testing
            semantic_caching: false,
            similarity_threshold: 0.0,
        };
        
        let cache = ResponseCache::new(config);
        
        // Store a response
        cache.store(
            "deepseek-coder",
            "What is Rust?",
            None,
            0.7,
            100,
            "Rust is a systems programming language.".to_string(),
        ).await.unwrap();
        
        // Immediate lookup should find it
        let response = cache.lookup(
            "deepseek-coder",
            "What is Rust?",
            None,
            0.7,
            100,
        ).await.unwrap();
        
        assert_eq!(response, Some("Rust is a systems programming language.".to_string()));
        
        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Now it should be gone
        let response = cache.lookup(
            "deepseek-coder",
            "What is Rust?",
            None,
            0.7,
            100,
        ).await.unwrap();
        
        assert_eq!(response, None);
    }
    
    #[tokio::test]
    async fn test_lru_behavior() {
        let config = CacheConfig {
            enabled: true,
            max_entries: 2, // Tiny cache
            ttl: Duration::from_secs(3600),
            semantic_caching: false,
            similarity_threshold: 0.0,
        };
        
        let cache = ResponseCache::new(config);
        
        // Store 3 responses (exceeding capacity)
        for i in 1..=3 {
            let prompt = format!("Prompt {}", i);
            let response = format!("Response {}", i);
            
            cache.store(
                "deepseek-coder",
                &prompt,
                None,
                0.7,
                100,
                response,
            ).await.unwrap();
        }
        
        // The first one should be evicted
        let first_response = cache.lookup(
            "deepseek-coder",
            "Prompt 1",
            None,
            0.7,
            100,
        ).await.unwrap();
        
        assert_eq!(first_response, None);
        
        // The last two should still be there
        for i in 2..=3 {
            let prompt = format!("Prompt {}", i);
            let expected_response = format!("Response {}", i);
            
            let response = cache.lookup(
                "deepseek-coder",
                &prompt,
                None,
                0.7,
                100,
            ).await.unwrap();
            
            assert_eq!(response, Some(expected_response));
        }
    }
} 