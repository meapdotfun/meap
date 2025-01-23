//! Security module provides authentication and encryption for MEAP.
//! 
//! This module handles:
//! - Agent authentication
//! - Message encryption/decryption
//! - Key management
//! - Security policy enforcement

use crate::error::{Error, Result};
use ring::{aead, rand};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Represents different authentication methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    /// Token-based authentication
    Token(String),
    /// Public key authentication
    PublicKey(Vec<u8>),
    /// Custom authentication method
    Custom(String),
}

/// Security configuration for agents and connections.
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Required authentication method
    pub auth_method: AuthMethod,
    /// Whether to encrypt messages
    pub encrypt_messages: bool,
    /// Custom security policies
    pub policies: Vec<SecurityPolicy>,
}

/// Security policies that can be enforced.
#[derive(Debug, Clone)]
pub enum SecurityPolicy {
    /// Require authentication for all messages
    RequireAuth,
    /// Require encryption for all messages
    RequireEncryption,
    /// Allow connections only from specific domains
    AllowedDomains(Vec<String>),
    /// Custom security policy
    Custom(String),
}

/// Manages security operations for MEAP.
pub struct SecurityManager {
    config: SecurityConfig,
    key: aead::LessSafeKey,
}

impl SecurityManager {
    /// Creates a new security manager with the given configuration.
    pub fn new(config: SecurityConfig) -> Result<Self> {
        let rng = rand::SystemRandom::new();
        let key = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &[0; 32])
            .map_err(|_| Error::Security("Failed to create encryption key".into()))?;
        let key = aead::LessSafeKey::new(key);

        Ok(Self { config, key })
    }

    /// Authenticates an agent using the configured method.
    pub fn authenticate(&self, credentials: &AuthMethod) -> Result<()> {
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
            _ => Err(Error::Security("Unsupported authentication method".into())),
        }
    }

    /// Encrypts a message using the configured encryption method.
    pub fn encrypt(&self, data: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>> {
        if !self.config.encrypt_messages {
            return Ok(data.to_vec());
        }

        let nonce = aead::Nonce::assume_unique_for_key(*nonce);
        let aad = aead::Aad::empty();

        self.key
            .seal_in_place_append_tag(nonce, aad, data)
            .map_err(|_| Error::Security("Encryption failed".into()))
    }

    /// Decrypts a message using the configured encryption method.
    pub fn decrypt(&self, data: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>> {
        if !self.config.encrypt_messages {
            return Ok(data.to_vec());
        }

        let nonce = aead::Nonce::assume_unique_for_key(*nonce);
        let aad = aead::Aad::empty();

        let mut buffer = data.to_vec();
        self.key
            .open_in_place(nonce, aad, &mut buffer)
            .map_err(|_| Error::Security("Decryption failed".into()))?;

        Ok(buffer)
    }

    /// Enforces security policies on a connection or message.
    pub fn enforce_policies(&self, domain: &str) -> Result<()> {
        for policy in &self.config.policies {
            match policy {
                SecurityPolicy::RequireAuth => {
                    // Authentication is handled separately
                }
                SecurityPolicy::RequireEncryption => {
                    if !self.config.encrypt_messages {
                        return Err(Error::Security("Encryption required by policy".into()));
                    }
                }
                SecurityPolicy::AllowedDomains(domains) => {
                    if !domains.iter().any(|d| domain.ends_with(d)) {
                        return Err(Error::Security("Domain not allowed by policy".into()));
                    }
                }
                SecurityPolicy::Custom(_) => {
                    // Custom policies handled by implementation
                }
            }
        }
        Ok(())
    }
}
