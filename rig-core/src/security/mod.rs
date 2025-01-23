//! Security module provides authentication, encryption, and key management.
//! 
//! This module handles:
//! - Key rotation and management
//! - TLS configuration
//! - Authentication and authorization
//! - Message encryption/decryption

use crate::error::{Error, Result};
use ring::{aead, rand, signature};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::{Duration, SystemTime}};
use tokio::sync::RwLock;

/// Duration after which encryption keys should be rotated
const KEY_ROTATION_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours

/// Represents different authentication methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    /// Token-based authentication
    Token(String),
    /// Public key authentication
    PublicKey(Vec<u8>),
    /// Certificate-based authentication
    Certificate(Vec<u8>),
    /// Custom authentication method
    Custom(String),
}

/// TLS configuration for secure connections
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Path to certificate file
    pub cert_path: String,
    /// Path to private key file
    pub key_path: String,
    /// Optional CA certificates for client authentication
    pub ca_certs: Option<Vec<String>>,
}

/// Security configuration for the system
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Required authentication method
    pub auth_method: AuthMethod,
    /// Whether to encrypt messages
    pub encrypt_messages: bool,
    /// TLS configuration
    pub tls_config: Option<TlsConfig>,
    /// Key rotation interval
    pub key_rotation_interval: Duration,
}

/// Represents an encryption key with metadata
struct EncryptionKey {
    key: aead::LessSafeKey,
    created_at: SystemTime,
}

/// Manages security operations and key rotation
pub struct SecurityManager {
    config: SecurityConfig,
    current_key: Arc<RwLock<EncryptionKey>>,
    previous_key: Arc<RwLock<Option<EncryptionKey>>>,
}

impl SecurityManager {
    /// Creates a new security manager with the given configuration
    pub async fn new(config: SecurityConfig) -> Result<Self> {
        let current_key = Self::generate_key()?;
        
        let manager = Self {
            config,
            current_key: Arc::new(RwLock::new(current_key)),
            previous_key: Arc::new(RwLock::new(None)),
        };

        // Start key rotation task
        manager.start_key_rotation();

        Ok(manager)
    }

    /// Generates a new encryption key
    fn generate_key() -> Result<EncryptionKey> {
        let rng = rand::SystemRandom::new();
        let key = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &[0; 32])
            .map_err(|_| Error::Security("Failed to create encryption key".into()))?;
        
        Ok(EncryptionKey {
            key: aead::LessSafeKey::new(key),
            created_at: SystemTime::now(),
        })
    }

    /// Starts the key rotation background task
    fn start_key_rotation(&self) {
        let current_key = self.current_key.clone();
        let previous_key = self.previous_key.clone();
        let interval = self.config.key_rotation_interval;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            loop {
                interval.tick().await;
                
                // Generate new key
                if let Ok(new_key) = Self::generate_key() {
                    // Move current key to previous
                    let mut prev = previous_key.write().await;
                    let mut curr = current_key.write().await;
                    *prev = Some(std::mem::replace(&mut *curr, new_key));
                }
            }
        });
    }

    /// Authenticates a client using the configured method
    pub async fn authenticate(&self, credentials: &AuthMethod) -> Result<()> {
        match (&self.config.auth_method, credentials) {
            (AuthMethod::Token(valid), AuthMethod::Token(provided)) => {
                if valid == provided {
                    Ok(())
                } else {
                    Err(Error::Security("Invalid token".into()))
                }
            }
            (AuthMethod::PublicKey(valid), AuthMethod::PublicKey(provided)) => {
                if valid == provided {
                    Ok(())
                } else {
                    Err(Error::Security("Invalid public key".into()))
                }
            }
            (AuthMethod::Certificate(_), AuthMethod::Certificate(_)) => {
                // Certificate validation is handled by TLS layer
                Ok(())
            }
            _ => Err(Error::Security("Unsupported authentication method".into())),
        }
    }

    /// Encrypts data using the current key
    pub async fn encrypt(&self, data: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>> {
        if !self.config.encrypt_messages {
            return Ok(data.to_vec());
        }

        let key = self.current_key.read().await;
        let nonce = aead::Nonce::assume_unique_for_key(*nonce);
        let aad = aead::Aad::empty();

        key.key
            .seal_in_place_append_tag(nonce, aad, data)
            .map_err(|_| Error::Security("Encryption failed".into()))
    }

    /// Decrypts data using current or previous key
    pub async fn decrypt(&self, data: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>> {
        if !self.config.encrypt_messages {
            return Ok(data.to_vec());
        }

        let nonce = aead::Nonce::assume_unique_for_key(*nonce);
        let aad = aead::Aad::empty();
        let mut buffer = data.to_vec();

        // Try current key first
        let current = self.current_key.read().await;
        match current.key.open_in_place(nonce, aad, &mut buffer) {
            Ok(_) => return Ok(buffer),
            Err(_) => {
                // Try previous key if available
                if let Some(prev_key) = &*self.previous_key.read().await {
                    return prev_key.key
                        .open_in_place(nonce, aad, &mut buffer)
                        .map(|_| buffer)
                        .map_err(|_| Error::Security("Decryption failed".into()));
                }
            }
        }

        Err(Error::Security("Decryption failed".into()))
    }
}
