//! Response types for Deepseek API
//! Handles parsing and validation of API responses

use serde::{Deserialize, Serialize};
use crate::CodeAnalysis;

/// Response from code generation endpoint
#[derive(Debug, Deserialize)]
pub struct GenerateResponse {
    /// Generated code
    pub code: String,
    /// Number of tokens generated
    pub token_count: usize,
    /// Model used for generation
    pub model: String,
    /// Optional warning messages
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Response from code analysis endpoint
#[derive(Debug, Deserialize)]
pub struct AnalyzeResponse {
    /// Code analysis results
    pub analysis: CodeAnalysis,
    /// Time taken for analysis
    pub analysis_time: f32,
    /// Model confidence score
    pub confidence: f32,
}

/// Response from text processing endpoint
#[derive(Debug, Deserialize)]
pub struct TextResponse {
    /// Generated text
    pub text: String,
    /// Number of tokens in response
    pub token_count: usize,
    /// Whether response was truncated
    pub truncated: bool,
}

/// Error response from Deepseek API
#[derive(Debug, Deserialize)]
pub struct ErrorResponse {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Optional error details
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    /// Converts API error into MEAP error
    pub fn into_error(self) -> crate::error::Error {
        crate::error::Error::Protocol(format!(
            "Deepseek API error {}: {}",
            self.code,
            self.message
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_generate_response() {
        let json = r#"{
            "code": "fn main() {\n    println!(\"Hello\");\n}",
            "token_count": 42,
            "model": "deepseek-coder-33b-instruct",
            "warnings": []
        }"#;

        let response: GenerateResponse = serde_json::from_str(json).unwrap();
        assert!(!response.code.is_empty());
        assert_eq!(response.token_count, 42);
        assert!(response.warnings.is_empty());
    }

    #[test]
    fn test_parse_error_response() {
        let json = r#"{
            "code": "rate_limit_exceeded",
            "message": "Too many requests",
            "details": null
        }"#;

        let error: ErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(error.code, "rate_limit_exceeded");
        assert!(!error.message.is_empty());
    }
} 