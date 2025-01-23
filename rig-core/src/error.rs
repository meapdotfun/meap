//! Error handling for the MEAP protocol.
//! 
//! This module provides a centralized error type and result alias for all
//! MEAP operations. It includes specific error variants for different
//! failure modes that can occur during protocol operation.
//! 
//! # Examples
//! 
//! ```rust
//! use meap_core::error::{Error, Result};
//! 
//! fn validate_agent_id(id: &str) -> Result<()> {
//!     if id.is_empty() {
//!         return Err(Error::validation("Agent ID cannot be empty"));
//!     }
//!     Ok(())
//! }
//! ```

use thiserror::Error;
use std::io;

/// Comprehensive error type for MEAP operations.
#[derive(Error, Debug)]
pub enum Error {
    /// Protocol-level errors (message format, validation, etc)
    #[error("Protocol error: {0}")]
    Protocol(String),
    
    /// Connection and networking errors
    #[error("Connection error: {0}")]
    Connection(String),
    
    /// Message sending failures
    #[error("Send error: {0}")]
    Send(String),
    
    /// I/O operation failures
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// Security-related errors (authentication, encryption)
    #[error("Security error: {0}")]
    Security(String),
    
    /// Data validation errors
    #[error("Validation error: {0}")]
    Validation(String),

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Stream processing errors
    #[error("Stream error: {0}")]
    Stream(String),

    /// Database operation errors
    #[error("Database error: {0}")]
    Database(String),

    /// Agent-specific errors
    #[error("Agent error: {0}")]
    Agent(String),

    /// Catch-all for other errors
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Convenience type alias for Results with MEAP errors.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Creates a new protocol error with the given message.
    /// 
    /// # Arguments
    /// * `msg` - Error message
    /// 
    /// # Examples
    /// ```
    /// use meap_core::error::Error;
    /// 
    /// let err = Error::protocol("Invalid message format");
    /// ```
    pub fn protocol(msg: impl Into<String>) -> Self {
        Error::Protocol(msg.into())
    }

    /// Creates a new security error with the given message.
    /// 
    /// # Arguments
    /// * `msg` - Error message
    /// 
    /// # Examples
    /// ```
    /// use meap_core::error::Error;
    /// 
    /// let err = Error::security("Authentication failed");
    /// ```
    pub fn security(msg: impl Into<String>) -> Self {
        Error::Security(msg.into())
    }

    /// Creates a new validation error with the given message.
    /// 
    /// # Arguments
    /// * `msg` - Error message
    /// 
    /// # Examples
    /// ```
    /// use meap_core::error::Error;
    /// 
    /// let err = Error::validation("Invalid agent ID format");
    /// ```
    pub fn validation(msg: impl Into<String>) -> Self {
        Error::Validation(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = Error::protocol("test error");
        assert!(matches!(err, Error::Protocol(_)));

        let err = Error::security("test error");
        assert!(matches!(err, Error::Security(_)));

        let err = Error::validation("test error");
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn test_error_display() {
        let err = Error::protocol("test error");
        assert_eq!(err.to_string(), "Protocol error: test error");

        let err = Error::security("test error");
        assert_eq!(err.to_string(), "Security error: test error");
    }
}
