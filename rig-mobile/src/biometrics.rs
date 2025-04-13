//! Mobile app biometric authentication and device security
//! Handles biometric authentication, device security checks, and secure access to device features

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Biometric type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BiometricType {
    /// Fingerprint authentication
    Fingerprint,
    /// Face recognition
    Face,
    /// Iris recognition
    Iris,
    /// Voice recognition
    Voice,
    /// Multiple biometrics
    Multiple,
}

/// Authentication level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuthLevel {
    /// No authentication required
    None,
    /// Basic authentication (PIN, pattern)
    Basic,
    /// Biometric authentication
    Biometric,
    /// Strong authentication (biometric + PIN)
    Strong,
}

/// Authentication result
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuthResult {
    /// Authentication successful
    Success,
    /// Authentication failed
    Failed,
    /// Authentication cancelled
    Cancelled,
    /// Authentication not available
    NotAvailable,
    /// Authentication locked out
    LockedOut,
    /// Authentication error
    Error,
}

/// Device security status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SecurityStatus {
    /// Device is secure
    Secure,
    /// Device is compromised
    Compromised,
    /// Device is rooted/jailbroken
    Rooted,
    /// Device has outdated security
    Outdated,
    /// Device security unknown
    Unknown,
}

/// Biometric configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiometricConfig {
    /// Required authentication level
    pub required_level: AuthLevel,
    /// Allowed biometric types
    pub allowed_types: Vec<BiometricType>,
    /// Authentication timeout
    pub auth_timeout: Duration,
    /// Maximum authentication attempts
    pub max_attempts: u32,
    /// Lockout duration
    pub lockout_duration: Duration,
    /// Whether to require re-authentication
    pub require_reauth: bool,
    /// Re-authentication interval
    pub reauth_interval: Duration,
    /// Whether to check device security
    pub check_device_security: bool,
    /// Whether to enforce strong authentication
    pub enforce_strong_auth: bool,
}

impl Default for BiometricConfig {
    fn default() -> Self {
        Self {
            required_level: AuthLevel::Biometric,
            allowed_types: vec![BiometricType::Fingerprint, BiometricType::Face],
            auth_timeout: Duration::from_secs(30),
            max_attempts: 5,
            lockout_duration: Duration::from_secs(60),
            require_reauth: true,
            reauth_interval: Duration::from_secs(300), // 5 minutes
            check_device_security: true,
            enforce_strong_auth: false,
        }
    }
}

/// Authentication session
#[derive(Debug, Clone)]
pub struct AuthSession {
    /// Session ID
    pub id: String,
    /// Authentication level
    pub level: AuthLevel,
    /// Biometric type used
    pub biometric_type: Option<BiometricType>,
    /// Creation timestamp
    pub created_at: Instant,
    /// Last authentication timestamp
    pub last_auth: Instant,
    /// Expiration timestamp
    pub expires_at: Instant,
    /// Authentication attempts
    pub attempts: u32,
    /// Whether session is locked
    pub locked: bool,
    /// Lock expiration timestamp
    pub lock_expires_at: Option<Instant>,
}

/// Biometric manager
pub struct BiometricManager {
    /// Biometric configuration
    config: BiometricConfig,
    /// Active sessions
    sessions: Arc<RwLock<HashMap<String, AuthSession>>>,
    /// Platform-specific biometric handlers
    handlers: Arc<RwLock<HashMap<String, Box<dyn BiometricHandler>>>>,
    /// Device security status
    security_status: Arc<RwLock<SecurityStatus>>,
}

/// Biometric handler trait
#[async_trait::async_trait]
pub trait BiometricHandler: Send + Sync {
    /// Initialize the biometric handler
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Check if biometric authentication is available
    async fn is_available(&self, biometric_type: BiometricType) -> Result<bool, Box<dyn std::error::Error>>;
    
    /// Authenticate with biometric
    async fn authenticate(&self, biometric_type: BiometricType) -> Result<AuthResult, Box<dyn std::error::Error>>;
    
    /// Check device security status
    async fn check_security(&self) -> Result<SecurityStatus, Box<dyn std::error::Error>>;
    
    /// Get available biometric types
    async fn get_available_types(&self) -> Result<Vec<BiometricType>, Box<dyn std::error::Error>>;
    
    /// Cancel authentication
    async fn cancel_auth(&self) -> Result<(), Box<dyn std::error::Error>>;
}

impl BiometricManager {
    /// Create a new biometric manager
    pub fn new(config: BiometricConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            handlers: Arc::new(RwLock::new(HashMap::new())),
            security_status: Arc::new(RwLock::new(SecurityStatus::Unknown)),
        }
    }
    
    /// Register a biometric handler
    pub async fn register_handler<H>(&self, platform: &str, handler: H)
    where
        H: BiometricHandler + 'static,
    {
        let mut handlers = self.handlers.write().await;
        handlers.insert(platform.to_string(), Box::new(handler));
    }
    
    /// Create a new authentication session
    pub async fn create_session(&self, level: AuthLevel) -> Result<String, Box<dyn std::error::Error>> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Instant::now();
        
        let session = AuthSession {
            id: id.clone(),
            level,
            biometric_type: None,
            created_at: now,
            last_auth: now,
            expires_at: now + self.config.auth_timeout,
            attempts: 0,
            locked: false,
            lock_expires_at: None,
        };
        
        let mut sessions = self.sessions.write().await;
        sessions.insert(id.clone(), session);
        
        Ok(id)
    }
    
    /// Authenticate with biometric
    pub async fn authenticate(&self, session_id: &str, biometric_type: BiometricType) -> Result<AuthResult, Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        let mut sessions = self.sessions.write().await;
        
        let session = sessions.get_mut(session_id)
            .ok_or_else(|| "Session not found".into())?;
            
        // Check if session is locked
        if session.locked {
            if let Some(lock_expires_at) = session.lock_expires_at {
                if Instant::now() < lock_expires_at {
                    return Ok(AuthResult::LockedOut);
                } else {
                    session.locked = false;
                    session.lock_expires_at = None;
                }
            } else {
                session.locked = false;
            }
        }
        
        // Check if biometric type is allowed
        if !self.config.allowed_types.contains(&biometric_type) {
            return Ok(AuthResult::NotAvailable);
        }
        
        // Check device security if required
        if self.config.check_device_security {
            let security_status = self.security_status.read().await;
            if *security_status != SecurityStatus::Secure {
                return Ok(AuthResult::Failed);
            }
        }
        
        // Try to authenticate with handlers
        for handler in handlers.values() {
            match handler.authenticate(biometric_type).await {
                Ok(AuthResult::Success) => {
                    session.last_auth = Instant::now();
                    session.biometric_type = Some(biometric_type);
                    session.attempts = 0;
                    return Ok(AuthResult::Success);
                }
                Ok(AuthResult::Failed) => {
                    session.attempts += 1;
                    if session.attempts >= self.config.max_attempts {
                        session.locked = true;
                        session.lock_expires_at = Some(Instant::now() + self.config.lockout_duration);
                        return Ok(AuthResult::LockedOut);
                    }
                }
                Ok(result) => {
                    return Ok(result);
                }
                Err(e) => {
                    warn!("Error authenticating: {}", e);
                }
            }
        }
        
        Ok(AuthResult::Error)
    }
    
    /// Check if authentication is required
    pub async fn is_auth_required(&self, session_id: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let sessions = self.sessions.read().await;
        
        let session = sessions.get(session_id)
            .ok_or_else(|| "Session not found".into())?;
            
        // Check if re-authentication is required
        if self.config.require_reauth {
            if Instant::now() - session.last_auth > self.config.reauth_interval {
                return Ok(true);
            }
        }
        
        // Check if session is expired
        if Instant::now() > session.expires_at {
            return Ok(true);
        }
        
        // Check if session is locked
        if session.locked {
            return Ok(true);
        }
        
        Ok(false)
    }
    
    /// Check device security status
    pub async fn check_security(&self) -> Result<SecurityStatus, Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        let mut security_status = SecurityStatus::Unknown;
        
        for handler in handlers.values() {
            match handler.check_security().await {
                Ok(status) => {
                    security_status = status;
                    break;
                }
                Err(e) => {
                    warn!("Error checking security: {}", e);
                }
            }
        }
        
        let mut status = self.security_status.write().await;
        *status = security_status;
        
        Ok(security_status)
    }
    
    /// Get available biometric types
    pub async fn get_available_types(&self) -> Result<Vec<BiometricType>, Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        let mut available_types = Vec::new();
        
        for handler in handlers.values() {
            match handler.get_available_types().await {
                Ok(types) => {
                    available_types.extend(types);
                }
                Err(e) => {
                    warn!("Error getting available types: {}", e);
                }
            }
        }
        
        Ok(available_types)
    }
    
    /// Cancel authentication
    pub async fn cancel_auth(&self) -> Result<(), Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        
        for handler in handlers.values() {
            if let Err(e) = handler.cancel_auth().await {
                warn!("Error cancelling authentication: {}", e);
            }
        }
        
        Ok(())
    }
}

/// Example biometric handler implementation
pub struct ConsoleBiometricHandler;

#[async_trait::async_trait]
impl BiometricHandler for ConsoleBiometricHandler {
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing console biometric handler");
        Ok(())
    }
    
    async fn is_available(&self, biometric_type: BiometricType) -> Result<bool, Box<dyn std::error::Error>> {
        info!("Checking console biometric availability: {:?}", biometric_type);
        Ok(true)
    }
    
    async fn authenticate(&self, biometric_type: BiometricType) -> Result<AuthResult, Box<dyn std::error::Error>> {
        info!("Authenticating with console biometric: {:?}", biometric_type);
        Ok(AuthResult::Success)
    }
    
    async fn check_security(&self) -> Result<SecurityStatus, Box<dyn std::error::Error>> {
        info!("Checking console device security");
        Ok(SecurityStatus::Secure)
    }
    
    async fn get_available_types(&self) -> Result<Vec<BiometricType>, Box<dyn std::error::Error>> {
        info!("Getting console available biometric types");
        Ok(vec![BiometricType::Fingerprint, BiometricType::Face])
    }
    
    async fn cancel_auth(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Cancelling console authentication");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_biometric_operations() {
        let config = BiometricConfig::default();
        let manager = BiometricManager::new(config);
        
        // Register console handler
        manager.register_handler("console", ConsoleBiometricHandler).await;
        
        // Create session
        let session_id = manager.create_session(AuthLevel::Biometric).await.unwrap();
        
        // Authenticate
        let result = manager.authenticate(&session_id, BiometricType::Fingerprint).await.unwrap();
        assert_eq!(result, AuthResult::Success);
        
        // Check if auth is required
        let required = manager.is_auth_required(&session_id).await.unwrap();
        assert!(!required);
    }
    
    #[tokio::test]
    async fn test_security_check() {
        let config = BiometricConfig::default();
        let manager = BiometricManager::new(config);
        
        // Register console handler
        manager.register_handler("console", ConsoleBiometricHandler).await;
        
        // Check security
        let status = manager.check_security().await.unwrap();
        assert_eq!(status, SecurityStatus::Secure);
    }
} 