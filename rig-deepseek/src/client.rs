//! Deepseek API client implementation
//! Handles direct communication with Deepseek's API endpoints

use crate::{DeepseekConfig, CodeAnalysis, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

/// API endpoints for Deepseek services
const API_BASE: &str = "https://api.deepseek.com/v1";
const CODE_ENDPOINT: &str = "/code/generate";
const ANALYZE_ENDPOINT: &str = "/code/analyze"; 
const TEXT_ENDPOINT: &str = "/text/process";

/// Request body for code generation
#[derive(Debug, Serialize)]
pub struct GenerateRequest {
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub stream: bool,
}

/// Request body for code analysis
#[derive(Debug, Serialize)]
pub struct AnalyzeRequest {
    pub code: String,
    pub include_metrics: bool,
}

/// Client for making Deepseek API calls
pub struct DeepseekClient {
    config: DeepseekConfig,
    http_client: reqwest::Client,
}

impl DeepseekClient {
    pub fn new(config: DeepseekConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            http_client,
        }
    }

    /// Makes an authenticated request to Deepseek API
    async fn make_request<T: Serialize>(
        &self,
        endpoint: &str,
        body: &T,
    ) -> Result<reqwest::Response> {
        let url = format!("{}{}", API_BASE, endpoint);
        
        let response = self.http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(body)
            .send()
            .await
            .map_err(|e| {
                error!("API request failed: {}", e);
                crate::Error::Connection(format!("API request failed: {}", e))
            })?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            error!("API error: {}", error);
            return Err(crate::Error::Protocol(format!("API error: {}", error)));
        }

        Ok(response)
    }

    /// Generates code using Deepseek Coder
    pub async fn generate_code(&self, prompt: &str) -> Result<String> {
        let request = GenerateRequest {
            prompt: prompt.to_string(),
            max_tokens: self.config.max_tokens,
            temperature: self.config.temperature,
            stream: self.config.stream,
        };

        let response = self.make_request(CODE_ENDPOINT, &request).await?;
        let code = response.text().await.map_err(|e| {
            error!("Failed to read response: {}", e);
            crate::Error::Protocol(format!("Failed to read response: {}", e))
        })?;

        Ok(code)
    }

    /// Analyzes code using Deepseek Coder
    pub async fn analyze_code(&self, code: &str) -> Result<CodeAnalysis> {
        let request = AnalyzeRequest {
            code: code.to_string(),
            include_metrics: true,
        };

        let response = self.make_request(ANALYZE_ENDPOINT, &request).await?;
        let analysis = response.json().await.map_err(|e| {
            error!("Failed to parse analysis response: {}", e);
            crate::Error::Protocol(format!("Failed to parse analysis: {}", e))
        })?;

        Ok(analysis)
    }

    /// Processes text using Deepseek LLM
    pub async fn process_text(&self, text: &str) -> Result<String> {
        let request = serde_json::json!({
            "text": text,
            "max_tokens": self.config.max_tokens,
            "temperature": self.config.temperature,
        });

        let response = self.make_request(TEXT_ENDPOINT, &request).await?;
        let result = response.text().await.map_err(|e| {
            error!("Failed to read response: {}", e);
            crate::Error::Protocol(format!("Failed to read response: {}", e))
        })?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client() {
        let config = DeepseekConfig::default();
        let client = DeepseekClient::new(config);

        // These tests would need a valid API key to pass
        // Just testing client creation for now
        assert!(client.http_client.timeout().is_some());
    }
} 