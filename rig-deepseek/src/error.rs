//! Error types for Deepseek API integration
//! Provides custom error handling for Deepseek-specific failures

use meap_core::error::Error as CoreError;
use thiserror::Error;

/// Custom error types for Deepseek operations
#[derive(Error, Debug)]
pub enum DeepseekError {
    /// API authentication failed
    #[error("Authentication failed: {0}")]
    AuthError(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    /// Invalid request parameters
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Model-specific errors
    #[error("Model error: {0}")]
    ModelError(String),

    /// Connection/network errors
    #[error("Connection error: {0}")]
    ConnectionError(String),

    /// Response parsing errors
    #[error("Parse error: {0}")]
    ParseError(String),
}

impl From<DeepseekError> for CoreError {
    fn from(err: DeepseekError) -> Self {
        match err {
            DeepseekError::AuthError(msg) => CoreError::Security(msg),
            DeepseekError::RateLimit(msg) => CoreError::RateLimit(msg),
            DeepseekError::InvalidRequest(msg) => CoreError::Validation(msg),
            DeepseekError::ModelError(msg) => CoreError::Protocol(msg),
            DeepseekError::ConnectionError(msg) => CoreError::Connection(msg),
            DeepseekError::ParseError(msg) => CoreError::Serialization(msg),
        }
    }
}

/// Convenience Result type for Deepseek operations
pub type Result<T> = std::result::Result<T, DeepseekError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_conversion() {
        let deepseek_err = DeepseekError::AuthError("Invalid token".into());
        let core_err: CoreError = deepseek_err.into();
        
        match core_err {
            CoreError::Security(msg) => assert_eq!(msg, "Invalid token"),
            _ => panic!("Wrong error conversion"),
        }
    }

    #[test]
    fn test_error_display() {
        let err = DeepseekError::RateLimit("Too many requests".into());
        assert_eq!(
            err.to_string(),
            "Rate limit exceeded: Too many requests"
        );
    }
} 