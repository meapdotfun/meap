//! API request/response handling for Deepseek
//! Manages request construction and response parsing

use crate::{
    error::{DeepseekError, Result},
    response::{GenerateResponse, AnalyzeResponse, TextResponse, ErrorResponse},
};
use serde::Serialize;
use reqwest::{Client, Response, StatusCode};
use tracing::{debug, error, info};

/// API configuration
const API_VERSION: &str = "v1";
const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// API request builder
#[derive(Debug)]
pub struct ApiRequest<T> {
    endpoint: String,
    body: T,
    timeout: Option<std::time::Duration>,
    retry_count: u32,
}

impl<T: Serialize> ApiRequest<T> {
    pub fn new(endpoint: impl Into<String>, body: T) -> Self {
        Self {
            endpoint: endpoint.into(),
            body,
            timeout: Some(DEFAULT_TIMEOUT),
            retry_count: 2,
        }
    }

    pub fn timeout(mut self, duration: std::time::Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    pub fn retries(mut self, count: u32) -> Self {
        self.retry_count = count;
        self
    }

    /// Executes the API request with retries
    pub async fn execute(
        self,
        client: &Client,
        api_key: &str,
    ) -> Result<Response> {
        let mut last_error = None;

        for attempt in 0..=self.retry_count {
            if attempt > 0 {
                debug!("Retrying request (attempt {})", attempt);
                tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
            }

            match self.try_request(client, api_key).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    error!("Request failed: {}", e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            DeepseekError::ConnectionError("Max retries exceeded".into())
        }))
    }

    async fn try_request(&self, client: &Client, api_key: &str) -> Result<Response> {
        let response = client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&self.body)
            .timeout(self.timeout.unwrap_or(DEFAULT_TIMEOUT))
            .send()
            .await
            .map_err(|e| DeepseekError::ConnectionError(e.to_string()))?;

        match response.status() {
            StatusCode::OK => Ok(response),
            StatusCode::TOO_MANY_REQUESTS => {
                Err(DeepseekError::RateLimit("Rate limit exceeded".into()))
            }
            StatusCode::UNAUTHORIZED => {
                Err(DeepseekError::AuthError("Invalid API key".into()))
            }
            _ => {
                let error: ErrorResponse = response
                    .json()
                    .await
                    .map_err(|e| DeepseekError::ParseError(e.to_string()))?;
                Err(error.into())
            }
        }
    }
}

/// Response parser for API responses
pub struct ApiResponse;

impl ApiResponse {
    /// Parses a code generation response
    pub async fn parse_generate(response: Response) -> Result<GenerateResponse> {
        response
            .json()
            .await
            .map_err(|e| DeepseekError::ParseError(format!("Failed to parse response: {}", e)))
    }

    /// Parses a code analysis response
    pub async fn parse_analyze(response: Response) -> Result<AnalyzeResponse> {
        response
            .json()
            .await
            .map_err(|e| DeepseekError::ParseError(format!("Failed to parse response: {}", e)))
    }

    /// Parses a text processing response
    pub async fn parse_text(response: Response) -> Result<TextResponse> {
        response
            .json()
            .await
            .map_err(|e| DeepseekError::ParseError(format!("Failed to parse response: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_request_builder() {
        let request = ApiRequest::new(
            "test_endpoint",
            json!({"test": "data"}),
        )
        .timeout(std::time::Duration::from_secs(60))
        .retries(3);

        assert_eq!(request.endpoint, "test_endpoint");
        assert_eq!(request.retry_count, 3);
        assert_eq!(request.timeout.unwrap().as_secs(), 60);
    }
} 