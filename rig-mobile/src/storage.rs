//! Mobile app data persistence and caching
//! Handles local storage, caching strategies, and data synchronization

use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

/// Cache entry with metadata
#[derive(Debug, Clone)]
pub struct CacheEntry<T> {
    /// Cached value
    pub value: T,
    /// Creation timestamp
    pub created_at: Instant,
    /// Last access timestamp
    pub last_accessed: Instant,
    /// Expiration time
    pub expires_at: Option<Instant>,
    /// Access count
    pub access_count: u64,
}

/// Cache eviction policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EvictionPolicy {
    /// Least recently used
    LRU,
    /// First in, first out
    FIFO,
    /// Least frequently used
    LFU,
    /// Random replacement
    Random,
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Maximum cache size in entries
    pub max_cache_size: usize,
    /// Default cache entry TTL
    pub default_ttl: Duration,
    /// Cache eviction policy
    pub eviction_policy: EvictionPolicy,
    /// Whether to enable persistence
    pub enable_persistence: bool,
    /// Whether to enable compression
    pub enable_compression: bool,
    /// Whether to enable encryption
    pub enable_encryption: bool,
    /// Background sync interval
    pub sync_interval: Duration,
    /// Maximum number of retries for failed operations
    pub max_retries: u32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            max_cache_size: 1000,
            default_ttl: Duration::from_secs(24 * 60 * 60), // 24 hours
            eviction_policy: EvictionPolicy::LRU,
            enable_persistence: true,
            enable_compression: true,
            enable_encryption: false,
            sync_interval: Duration::from_secs(300), // 5 minutes
            max_retries: 3,
        }
    }
}

/// Storage operation result
#[derive(Debug)]
pub enum StorageResult<T> {
    /// Operation successful with value
    Success(T),
    /// Value not found
    NotFound,
    /// Value expired
    Expired,
    /// Operation failed with error
    Error(Box<dyn std::error::Error>),
}

/// Storage manager
pub struct StorageManager {
    /// Storage configuration
    config: StorageConfig,
    /// In-memory cache
    cache: Arc<RwLock<HashMap<String, CacheEntry<Vec<u8>>>>,
    /// Background task channel
    background_tx: mpsc::Sender<BackgroundTask>,
    /// Platform-specific storage handlers
    handlers: Arc<RwLock<HashMap<String, Box<dyn StorageHandler>>>>,
    /// Operation queue for persistence
    operation_queue: Arc<RwLock<VecDeque<StorageOperation>>>,
}

/// Background task types
#[derive(Debug)]
pub enum BackgroundTask {
    /// Persist queued operations
    PersistQueue,
    /// Clean up expired entries
    Cleanup,
    /// Sync with remote storage
    Sync,
}

/// Storage operation
#[derive(Debug)]
pub enum StorageOperation {
    /// Set value
    Set {
        key: String,
        value: Vec<u8>,
        ttl: Option<Duration>,
    },
    /// Delete value
    Delete(String),
    /// Clear all values
    Clear,
}

/// Storage handler trait
#[async_trait::async_trait]
pub trait StorageHandler: Send + Sync {
    /// Initialize the storage handler
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Read value from storage
    async fn read(&self, key: &str) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>>;
    
    /// Write value to storage
    async fn write(&self, key: &str, value: &[u8]) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Delete value from storage
    async fn delete(&self, key: &str) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Clear all values
    async fn clear(&self) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Sync with remote storage
    async fn sync(&self) -> Result<(), Box<dyn std::error::Error>>;
}

impl StorageManager {
    /// Create a new storage manager
    pub fn new(config: StorageConfig) -> Self {
        let (background_tx, background_rx) = mpsc::channel(100);
        
        let manager = Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
            background_tx,
            handlers: Arc::new(RwLock::new(HashMap::new())),
            operation_queue: Arc::new(RwLock::new(VecDeque::new())),
        };
        
        // Start background task
        tokio::spawn(manager.run_background_task(background_rx));
        
        manager
    }
    
    /// Register a storage handler
    pub async fn register_handler<H>(&self, platform: &str, handler: H)
    where
        H: StorageHandler + 'static,
    {
        let mut handlers = self.handlers.write().await;
        handlers.insert(platform.to_string(), Box::new(handler));
    }
    
    /// Get value from storage
    pub async fn get<T>(&self, key: &str) -> StorageResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(key) {
                // Check expiration
                if let Some(expires_at) = entry.expires_at {
                    if expires_at < Instant::now() {
                        return StorageResult::Expired;
                    }
                }
                
                // Update access metadata
                let mut cache = self.cache.write().await;
                if let Some(entry) = cache.get_mut(key) {
                    entry.last_accessed = Instant::now();
                    entry.access_count += 1;
                }
                
                // Deserialize and return value
                match serde_json::from_slice(&entry.value) {
                    Ok(value) => return StorageResult::Success(value),
                    Err(e) => return StorageResult::Error(Box::new(e)),
                }
            }
        }
        
        // Try persistent storage
        if self.config.enable_persistence {
            let handlers = self.handlers.read().await;
            for handler in handlers.values() {
                match handler.read(key).await {
                    Ok(Some(value)) => {
                        // Cache the value
                        self.cache_value(key, &value, None).await;
                        
                        // Deserialize and return
                        match serde_json::from_slice(&value) {
                            Ok(value) => return StorageResult::Success(value),
                            Err(e) => return StorageResult::Error(Box::new(e)),
                        }
                    }
                    Ok(None) => continue,
                    Err(e) => warn!("Error reading from storage: {}", e),
                }
            }
        }
        
        StorageResult::NotFound
    }
    
    /// Set value in storage
    pub async fn set<T>(&self, key: &str, value: &T, ttl: Option<Duration>) -> Result<(), Box<dyn std::error::Error>>
    where
        T: Serialize,
    {
        // Serialize value
        let value = serde_json::to_vec(value)?;
        
        // Cache the value
        self.cache_value(key, &value, ttl).await;
        
        // Queue for persistence
        if self.config.enable_persistence {
            let mut queue = self.operation_queue.write().await;
            queue.push_back(StorageOperation::Set {
                key: key.to_string(),
                value: value.clone(),
                ttl,
            });
            
            // Trigger persistence
            self.background_tx.send(BackgroundTask::PersistQueue).await?;
        }
        
        Ok(())
    }
    
    /// Delete value from storage
    pub async fn delete(&self, key: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Remove from cache
        {
            let mut cache = self.cache.write().await;
            cache.remove(key);
        }
        
        // Queue for persistence
        if self.config.enable_persistence {
            let mut queue = self.operation_queue.write().await;
            queue.push_back(StorageOperation::Delete(key.to_string()));
            
            // Trigger persistence
            self.background_tx.send(BackgroundTask::PersistQueue).await?;
        }
        
        Ok(())
    }
    
    /// Clear all values
    pub async fn clear(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Clear cache
        {
            let mut cache = self.cache.write().await;
            cache.clear();
        }
        
        // Queue for persistence
        if self.config.enable_persistence {
            let mut queue = self.operation_queue.write().await;
            queue.push_back(StorageOperation::Clear);
            
            // Trigger persistence
            self.background_tx.send(BackgroundTask::PersistQueue).await?;
        }
        
        Ok(())
    }
    
    /// Cache a value
    async fn cache_value(&self, key: &str, value: &[u8], ttl: Option<Duration>) {
        let mut cache = self.cache.write().await;
        
        // Check cache size
        if cache.len() >= self.config.max_cache_size {
            self.evict_entry(&mut cache).await;
        }
        
        // Create cache entry
        let now = Instant::now();
        let entry = CacheEntry {
            value: value.to_vec(),
            created_at: now,
            last_accessed: now,
            expires_at: ttl.map(|d| now + d),
            access_count: 0,
        };
        
        cache.insert(key.to_string(), entry);
    }
    
    /// Evict an entry based on policy
    async fn evict_entry(&self, cache: &mut HashMap<String, CacheEntry<Vec<u8>>>) {
        match self.config.eviction_policy {
            EvictionPolicy::LRU => {
                if let Some(key) = cache.iter()
                    .min_by_key(|(_, entry)| entry.last_accessed)
                    .map(|(key, _)| key.clone())
                {
                    cache.remove(&key);
                }
            }
            EvictionPolicy::FIFO => {
                if let Some(key) = cache.iter()
                    .min_by_key(|(_, entry)| entry.created_at)
                    .map(|(key, _)| key.clone())
                {
                    cache.remove(&key);
                }
            }
            EvictionPolicy::LFU => {
                if let Some(key) = cache.iter()
                    .min_by_key(|(_, entry)| entry.access_count)
                    .map(|(key, _)| key.clone())
                {
                    cache.remove(&key);
                }
            }
            EvictionPolicy::Random => {
                if let Some(key) = cache.keys().next().cloned() {
                    cache.remove(&key);
                }
            }
        }
    }
    
    /// Run background task
    async fn run_background_task(&self, mut rx: mpsc::Receiver<BackgroundTask>) {
        let mut interval = tokio::time::interval(self.config.sync_interval);
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Clean up expired entries
                    if let Err(e) = self.cleanup_expired().await {
                        warn!("Error cleaning up expired entries: {}", e);
                    }
                    
                    // Sync with remote storage
                    if let Err(e) = self.sync_storage().await {
                        warn!("Error syncing storage: {}", e);
                    }
                }
                
                Some(task) = rx.recv() => {
                    match task {
                        BackgroundTask::PersistQueue => {
                            if let Err(e) = self.persist_queue().await {
                                warn!("Error persisting queue: {}", e);
                            }
                        }
                        BackgroundTask::Cleanup => {
                            if let Err(e) = self.cleanup_expired().await {
                                warn!("Error cleaning up expired entries: {}", e);
                            }
                        }
                        BackgroundTask::Sync => {
                            if let Err(e) = self.sync_storage().await {
                                warn!("Error syncing storage: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }
    
    /// Persist queued operations
    async fn persist_queue(&self) -> Result<(), Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        let mut queue = self.operation_queue.write().await;
        
        while let Some(operation) = queue.pop_front() {
            for handler in handlers.values() {
                match &operation {
                    StorageOperation::Set { key, value, .. } => {
                        if let Err(e) = handler.write(key, value).await {
                            warn!("Error writing to storage: {}", e);
                            queue.push_front(operation);
                            break;
                        }
                    }
                    StorageOperation::Delete(key) => {
                        if let Err(e) = handler.delete(key).await {
                            warn!("Error deleting from storage: {}", e);
                            queue.push_front(operation);
                            break;
                        }
                    }
                    StorageOperation::Clear => {
                        if let Err(e) = handler.clear().await {
                            warn!("Error clearing storage: {}", e);
                            queue.push_front(operation);
                            break;
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Clean up expired entries
    async fn cleanup_expired(&self) -> Result<(), Box<dyn std::error::Error>> {
        let now = Instant::now();
        let mut cache = self.cache.write().await;
        
        cache.retain(|_, entry| {
            entry.expires_at.map_or(true, |expires_at| expires_at > now)
        });
        
        Ok(())
    }
    
    /// Sync with remote storage
    async fn sync_storage(&self) -> Result<(), Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        
        for handler in handlers.values() {
            if let Err(e) = handler.sync().await {
                warn!("Error syncing storage: {}", e);
            }
        }
        
        Ok(())
    }
}

/// Example storage handler implementation
pub struct ConsoleStorageHandler;

#[async_trait::async_trait]
impl StorageHandler for ConsoleStorageHandler {
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing console storage handler");
        Ok(())
    }
    
    async fn read(&self, key: &str) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
        info!("Reading from console storage: {}", key);
        Ok(None)
    }
    
    async fn write(&self, key: &str, value: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        info!("Writing to console storage: {}", key);
        Ok(())
    }
    
    async fn delete(&self, key: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("Deleting from console storage: {}", key);
        Ok(())
    }
    
    async fn clear(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Clearing console storage");
        Ok(())
    }
    
    async fn sync(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Syncing console storage");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_storage_operations() {
        let config = StorageConfig::default();
        let manager = StorageManager::new(config);
        
        // Register console handler
        manager.register_handler("console", ConsoleStorageHandler).await;
        
        // Test value
        let test_value = "test_value".to_string();
        
        // Set value
        manager.set("test_key", &test_value, None).await.unwrap();
        
        // Get value
        match manager.get::<String>("test_key").await {
            StorageResult::Success(value) => assert_eq!(value, test_value),
            _ => panic!("Expected success"),
        }
        
        // Delete value
        manager.delete("test_key").await.unwrap();
        
        // Check deletion
        match manager.get::<String>("test_key").await {
            StorageResult::NotFound => (),
            _ => panic!("Expected not found"),
        }
    }
    
    #[tokio::test]
    async fn test_cache_expiration() {
        let config = StorageConfig::default();
        let manager = StorageManager::new(config);
        
        // Test value with short TTL
        let test_value = "test_value".to_string();
        let ttl = Duration::from_millis(100);
        
        // Set value with TTL
        manager.set("test_key", &test_value, Some(ttl)).await.unwrap();
        
        // Wait for expiration
        tokio::time::sleep(ttl).await;
        
        // Check expiration
        match manager.get::<String>("test_key").await {
            StorageResult::Expired => (),
            _ => panic!("Expected expired"),
        }
    }
} 