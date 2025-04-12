//! Mobile app security and encryption
//! Handles secure storage, encryption/decryption, and key management

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Encryption algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EncryptionAlgorithm {
    /// AES-256-GCM
    Aes256Gcm,
    /// ChaCha20-Poly1305
    ChaCha20Poly1305,
    /// XChaCha20-Poly1305
    XChaCha20Poly1305,
}

/// Key type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyType {
    /// Master key for key derivation
    Master,
    /// Data encryption key
    Data,
    /// Authentication key
    Auth,
    /// Backup key
    Backup,
}

/// Key metadata
#[derive(Debug, Clone)]
pub struct KeyMetadata {
    /// Key type
    pub key_type: KeyType,
    /// Creation timestamp
    pub created_at: Instant,
    /// Last rotation timestamp
    pub rotated_at: Option<Instant>,
    /// Expiration timestamp
    pub expires_at: Option<Instant>,
    /// Usage count
    pub usage_count: u64,
    /// Version
    pub version: u32,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Default encryption algorithm
    pub default_algorithm: EncryptionAlgorithm,
    /// Key rotation interval
    pub key_rotation_interval: Duration,
    /// Key expiration interval
    pub key_expiration_interval: Duration,
    /// Maximum key usage count
    pub max_key_usage: u64,
    /// Whether to enable secure storage
    pub enable_secure_storage: bool,
    /// Secure storage path
    pub secure_storage_path: String,
    /// Whether to enable key backup
    pub enable_key_backup: bool,
    /// Backup encryption key
    pub backup_key: Option<Vec<u8>>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            default_algorithm: EncryptionAlgorithm::Aes256Gcm,
            key_rotation_interval: Duration::from_secs(24 * 60 * 60), // 24 hours
            key_expiration_interval: Duration::from_secs(7 * 24 * 60 * 60), // 7 days
            max_key_usage: 1000,
            enable_secure_storage: true,
            secure_storage_path: "secure_storage".to_string(),
            enable_key_backup: false,
            backup_key: None,
        }
    }
}

/// Security manager
pub struct SecurityManager {
    /// Security configuration
    config: SecurityConfig,
    /// Key storage
    keys: Arc<RwLock<HashMap<String, (Vec<u8>, KeyMetadata)>>>,
    /// Secure storage
    secure_storage: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    /// Platform-specific security handlers
    handlers: Arc<RwLock<HashMap<String, Box<dyn SecurityHandler>>>>,
}

/// Security handler trait
#[async_trait::async_trait]
pub trait SecurityHandler: Send + Sync {
    /// Initialize the security handler
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Generate a new key
    async fn generate_key(&self, key_type: KeyType) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
    
    /// Encrypt data
    async fn encrypt(&self, data: &[u8], key: &[u8], algorithm: EncryptionAlgorithm) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
    
    /// Decrypt data
    async fn decrypt(&self, data: &[u8], key: &[u8], algorithm: EncryptionAlgorithm) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
    
    /// Store data securely
    async fn store_secure(&self, key: &str, data: &[u8]) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Retrieve data securely
    async fn retrieve_secure(&self, key: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
    
    /// Backup keys
    async fn backup_keys(&self, keys: &[(String, Vec<u8>, KeyMetadata)], backup_key: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
    
    /// Restore keys from backup
    async fn restore_keys(&self, backup_data: &[u8], backup_key: &[u8]) -> Result<Vec<(String, Vec<u8>, KeyMetadata)>, Box<dyn std::error::Error>>;
}

impl SecurityManager {
    /// Create a new security manager
    pub fn new(config: SecurityConfig) -> Self {
        Self {
            config,
            keys: Arc::new(RwLock::new(HashMap::new())),
            secure_storage: Arc::new(RwLock::new(HashMap::new())),
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Register a security handler
    pub async fn register_handler<H>(&self, platform: &str, handler: H)
    where
        H: SecurityHandler + 'static,
    {
        let mut handlers = self.handlers.write().await;
        handlers.insert(platform.to_string(), Box::new(handler));
    }
    
    /// Generate a new key
    pub async fn generate_key(&self, key_type: KeyType) -> Result<String, Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        let mut key_id = None;
        
        for handler in handlers.values() {
            match handler.generate_key(key_type).await {
                Ok(key) => {
                    let id = uuid::Uuid::new_v4().to_string();
                    let metadata = KeyMetadata {
                        key_type,
                        created_at: Instant::now(),
                        rotated_at: None,
                        expires_at: Some(Instant::now() + self.config.key_expiration_interval),
                        usage_count: 0,
                        version: 1,
                    };
                    
                    let mut keys = self.keys.write().await;
                    keys.insert(id.clone(), (key, metadata));
                    key_id = Some(id);
                    break;
                }
                Err(e) => {
                    warn!("Error generating key: {}", e);
                }
            }
        }
        
        key_id.ok_or_else(|| "Failed to generate key".into())
    }
    
    /// Encrypt data
    pub async fn encrypt(&self, data: &[u8], key_id: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        let keys = self.keys.read().await;
        
        let (key, metadata) = keys.get(key_id)
            .ok_or_else(|| "Key not found".into())?;
            
        // Check key expiration
        if let Some(expires_at) = metadata.expires_at {
            if Instant::now() > expires_at {
                return Err("Key has expired".into());
            }
        }
        
        // Check key usage
        if metadata.usage_count >= self.config.max_key_usage {
            return Err("Key usage limit exceeded".into());
        }
        
        // Try to encrypt with handlers
        for handler in handlers.values() {
            match handler.encrypt(data, key, self.config.default_algorithm).await {
                Ok(encrypted) => {
                    // Update key usage
                    let mut keys = self.keys.write().await;
                    if let Some((_, metadata)) = keys.get_mut(key_id) {
                        metadata.usage_count += 1;
                    }
                    return Ok(encrypted);
                }
                Err(e) => {
                    warn!("Error encrypting data: {}", e);
                }
            }
        }
        
        Err("Failed to encrypt data".into())
    }
    
    /// Decrypt data
    pub async fn decrypt(&self, data: &[u8], key_id: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        let keys = self.keys.read().await;
        
        let (key, metadata) = keys.get(key_id)
            .ok_or_else(|| "Key not found".into())?;
            
        // Check key expiration
        if let Some(expires_at) = metadata.expires_at {
            if Instant::now() > expires_at {
                return Err("Key has expired".into());
            }
        }
        
        // Try to decrypt with handlers
        for handler in handlers.values() {
            match handler.decrypt(data, key, self.config.default_algorithm).await {
                Ok(decrypted) => {
                    return Ok(decrypted);
                }
                Err(e) => {
                    warn!("Error decrypting data: {}", e);
                }
            }
        }
        
        Err("Failed to decrypt data".into())
    }
    
    /// Store data securely
    pub async fn store_secure(&self, key: &str, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        if !self.config.enable_secure_storage {
            return Err("Secure storage is disabled".into());
        }
        
        let handlers = self.handlers.read().await;
        
        // Try to store with handlers
        for handler in handlers.values() {
            match handler.store_secure(key, data).await {
                Ok(_) => {
                    let mut storage = self.secure_storage.write().await;
                    storage.insert(key.to_string(), data.to_vec());
                    return Ok(());
                }
                Err(e) => {
                    warn!("Error storing data securely: {}", e);
                }
            }
        }
        
        Err("Failed to store data securely".into())
    }
    
    /// Retrieve data securely
    pub async fn retrieve_secure(&self, key: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        if !self.config.enable_secure_storage {
            return Err("Secure storage is disabled".into());
        }
        
        let handlers = self.handlers.read().await;
        
        // Try to retrieve with handlers
        for handler in handlers.values() {
            match handler.retrieve_secure(key).await {
                Ok(data) => {
                    return Ok(data);
                }
                Err(e) => {
                    warn!("Error retrieving data securely: {}", e);
                }
            }
        }
        
        Err("Failed to retrieve data securely".into())
    }
    
    /// Backup keys
    pub async fn backup_keys(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        if !self.config.enable_key_backup {
            return Err("Key backup is disabled".into());
        }
        
        let backup_key = self.config.backup_key.as_ref()
            .ok_or_else(|| "Backup key not set".into())?;
            
        let handlers = self.handlers.read().await;
        let keys = self.keys.read().await;
        
        let key_data: Vec<_> = keys.iter()
            .map(|(id, (key, metadata))| (id.clone(), key.clone(), metadata.clone()))
            .collect();
        
        // Try to backup with handlers
        for handler in handlers.values() {
            match handler.backup_keys(&key_data, backup_key).await {
                Ok(backup) => {
                    return Ok(backup);
                }
                Err(e) => {
                    warn!("Error backing up keys: {}", e);
                }
            }
        }
        
        Err("Failed to backup keys".into())
    }
    
    /// Restore keys from backup
    pub async fn restore_keys(&self, backup_data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        if !self.config.enable_key_backup {
            return Err("Key backup is disabled".into());
        }
        
        let backup_key = self.config.backup_key.as_ref()
            .ok_or_else(|| "Backup key not set".into())?;
            
        let handlers = self.handlers.read().await;
        
        // Try to restore with handlers
        for handler in handlers.values() {
            match handler.restore_keys(backup_data, backup_key).await {
                Ok(keys) => {
                    let mut key_storage = self.keys.write().await;
                    for (id, key, metadata) in keys {
                        key_storage.insert(id, (key, metadata));
                    }
                    return Ok(());
                }
                Err(e) => {
                    warn!("Error restoring keys: {}", e);
                }
            }
        }
        
        Err("Failed to restore keys".into())
    }
}

/// Example security handler implementation
pub struct ConsoleSecurityHandler;

#[async_trait::async_trait]
impl SecurityHandler for ConsoleSecurityHandler {
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing console security handler");
        Ok(())
    }
    
    async fn generate_key(&self, key_type: KeyType) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        info!("Generating console key: {:?}", key_type);
        Ok(vec![0; 32])
    }
    
    async fn encrypt(&self, data: &[u8], key: &[u8], algorithm: EncryptionAlgorithm) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        info!("Encrypting console data with {:?}", algorithm);
        Ok(data.to_vec())
    }
    
    async fn decrypt(&self, data: &[u8], key: &[u8], algorithm: EncryptionAlgorithm) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        info!("Decrypting console data with {:?}", algorithm);
        Ok(data.to_vec())
    }
    
    async fn store_secure(&self, key: &str, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        info!("Storing console data securely: {}", key);
        Ok(())
    }
    
    async fn retrieve_secure(&self, key: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        info!("Retrieving console data securely: {}", key);
        Ok(vec![])
    }
    
    async fn backup_keys(&self, keys: &[(String, Vec<u8>, KeyMetadata)], backup_key: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        info!("Backing up console keys");
        Ok(vec![])
    }
    
    async fn restore_keys(&self, backup_data: &[u8], backup_key: &[u8]) -> Result<Vec<(String, Vec<u8>, KeyMetadata)>, Box<dyn std::error::Error>> {
        info!("Restoring console keys");
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_security_operations() {
        let config = SecurityConfig::default();
        let manager = SecurityManager::new(config);
        
        // Register console handler
        manager.register_handler("console", ConsoleSecurityHandler).await;
        
        // Generate key
        let key_id = manager.generate_key(KeyType::Data).await.unwrap();
        
        // Encrypt data
        let data = b"test data";
        let encrypted = manager.encrypt(data, &key_id).await.unwrap();
        
        // Decrypt data
        let decrypted = manager.decrypt(&encrypted, &key_id).await.unwrap();
        assert_eq!(data.to_vec(), decrypted);
    }
    
    #[tokio::test]
    async fn test_secure_storage() {
        let config = SecurityConfig::default();
        let manager = SecurityManager::new(config);
        
        // Register console handler
        manager.register_handler("console", ConsoleSecurityHandler).await;
        
        // Store data
        let data = b"secure data";
        manager.store_secure("test_key", data).await.unwrap();
        
        // Retrieve data
        let retrieved = manager.retrieve_secure("test_key").await.unwrap();
        assert_eq!(data.to_vec(), retrieved);
    }
} 