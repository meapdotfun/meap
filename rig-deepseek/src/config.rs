//! Configuration management for Deepseek integration
//! Handles API settings, model parameters, and environment configuration

use crate::error::{DeepseekError, Result};
use serde::{Deserialize, Serialize};
use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};
use tracing::{debug, info, warn};

/// API configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// API endpoint URL
    pub endpoint: String,
    /// API key
    pub api_key: String,
    /// Request timeout
    pub timeout: Duration,
    /// Maximum retries
    pub max_retries: u32,
    /// Retry delay
    pub retry_delay: Duration,
    /// Rate limit (requests per minute)
    pub rate_limit: u32,
}

/// Model configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Default model to use
    pub default_model: String,
    /// Maximum context length
    pub max_context_length: usize,
    /// Maximum sequence length
    pub max_sequence_length: usize,
    /// Default temperature
    pub default_temperature: f32,
    /// Default top-p value
    pub default_top_p: f32,
    /// Default top-k value
    pub default_top_k: u32,
    /// Default max tokens
    pub default_max_tokens: u32,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Log level (debug, info, warn, error)
    pub level: String,
    /// Whether to enable JSON logging
    pub json: bool,
    /// Log file path (if any)
    pub file: Option<PathBuf>,
}

/// Main configuration struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// API settings
    pub api: ApiConfig,
    /// Model settings
    pub model: ModelConfig,
    /// Logging settings
    pub log: LogConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api: ApiConfig {
                endpoint: "https://api.deepseek.com/v1".to_string(),
                api_key: String::new(),
                timeout: Duration::from_secs(30),
                max_retries: 3,
                retry_delay: Duration::from_secs(1),
                rate_limit: 60,
            },
            model: ModelConfig {
                default_model: "deepseek-coder-6.7b-base".to_string(),
                max_context_length: 8192,
                max_sequence_length: 2048,
                default_temperature: 0.7,
                default_top_p: 0.9,
                default_top_k: 40,
                default_max_tokens: 1024,
            },
            log: LogConfig {
                level: "info".to_string(),
                json: false,
                file: None,
            },
        }
    }
}

impl Config {
    /// Load configuration from a file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| DeepseekError::ConfigError(format!("Failed to read config file: {}", e)))?;
            
        let mut config: Config = serde_json::from_str(&contents)
            .map_err(|e| DeepseekError::ConfigError(format!("Failed to parse config: {}", e)))?;
            
        // Override with environment variables if present
        config.load_from_env()?;
        
        Ok(config)
    }
    
    /// Load configuration from environment variables
    pub fn load_from_env(&mut self) -> Result<()> {
        // API settings
        if let Ok(endpoint) = env::var("DEEPSEEK_API_ENDPOINT") {
            self.api.endpoint = endpoint;
        }
        
        if let Ok(api_key) = env::var("DEEPSEEK_API_KEY") {
            self.api.api_key = api_key;
        }
        
        if let Ok(timeout) = env::var("DEEPSEEK_TIMEOUT") {
            self.api.timeout = Duration::from_secs(
                timeout.parse()
                    .map_err(|e| DeepseekError::ConfigError(format!("Invalid timeout: {}", e)))?
            );
        }
        
        if let Ok(max_retries) = env::var("DEEPSEEK_MAX_RETRIES") {
            self.api.max_retries = max_retries.parse()
                .map_err(|e| DeepseekError::ConfigError(format!("Invalid max retries: {}", e)))?;
        }
        
        if let Ok(retry_delay) = env::var("DEEPSEEK_RETRY_DELAY") {
            self.api.retry_delay = Duration::from_secs(
                retry_delay.parse()
                    .map_err(|e| DeepseekError::ConfigError(format!("Invalid retry delay: {}", e)))?
            );
        }
        
        if let Ok(rate_limit) = env::var("DEEPSEEK_RATE_LIMIT") {
            self.api.rate_limit = rate_limit.parse()
                .map_err(|e| DeepseekError::ConfigError(format!("Invalid rate limit: {}", e)))?;
        }
        
        // Model settings
        if let Ok(default_model) = env::var("DEEPSEEK_DEFAULT_MODEL") {
            self.model.default_model = default_model;
        }
        
        if let Ok(max_context) = env::var("DEEPSEEK_MAX_CONTEXT_LENGTH") {
            self.model.max_context_length = max_context.parse()
                .map_err(|e| DeepseekError::ConfigError(format!("Invalid max context length: {}", e)))?;
        }
        
        if let Ok(max_sequence) = env::var("DEEPSEEK_MAX_SEQUENCE_LENGTH") {
            self.model.max_sequence_length = max_sequence.parse()
                .map_err(|e| DeepseekError::ConfigError(format!("Invalid max sequence length: {}", e)))?;
        }
        
        if let Ok(temperature) = env::var("DEEPSEEK_DEFAULT_TEMPERATURE") {
            self.model.default_temperature = temperature.parse()
                .map_err(|e| DeepseekError::ConfigError(format!("Invalid temperature: {}", e)))?;
        }
        
        if let Ok(top_p) = env::var("DEEPSEEK_DEFAULT_TOP_P") {
            self.model.default_top_p = top_p.parse()
                .map_err(|e| DeepseekError::ConfigError(format!("Invalid top-p: {}", e)))?;
        }
        
        if let Ok(top_k) = env::var("DEEPSEEK_DEFAULT_TOP_K") {
            self.model.default_top_k = top_k.parse()
                .map_err(|e| DeepseekError::ConfigError(format!("Invalid top-k: {}", e)))?;
        }
        
        if let Ok(max_tokens) = env::var("DEEPSEEK_DEFAULT_MAX_TOKENS") {
            self.model.default_max_tokens = max_tokens.parse()
                .map_err(|e| DeepseekError::ConfigError(format!("Invalid max tokens: {}", e)))?;
        }
        
        // Logging settings
        if let Ok(level) = env::var("DEEPSEEK_LOG_LEVEL") {
            self.log.level = level;
        }
        
        if let Ok(json) = env::var("DEEPSEEK_LOG_JSON") {
            self.log.json = json.parse()
                .map_err(|e| DeepseekError::ConfigError(format!("Invalid log json setting: {}", e)))?;
        }
        
        if let Ok(file) = env::var("DEEPSEEK_LOG_FILE") {
            self.log.file = Some(PathBuf::from(file));
        }
        
        Ok(())
    }
    
    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Validate API settings
        if self.api.api_key.is_empty() {
            warn!("No API key provided - some features may be limited");
        }
        
        if self.api.timeout.as_secs() == 0 {
            return Err(DeepseekError::ConfigError("Timeout must be greater than 0".to_string()));
        }
        
        if self.api.max_retries == 0 {
            return Err(DeepseekError::ConfigError("Max retries must be greater than 0".to_string()));
        }
        
        if self.api.rate_limit == 0 {
            return Err(DeepseekError::ConfigError("Rate limit must be greater than 0".to_string()));
        }
        
        // Validate model settings
        if self.model.max_context_length == 0 {
            return Err(DeepseekError::ConfigError("Max context length must be greater than 0".to_string()));
        }
        
        if self.model.max_sequence_length == 0 {
            return Err(DeepseekError::ConfigError("Max sequence length must be greater than 0".to_string()));
        }
        
        if self.model.max_sequence_length > self.model.max_context_length {
            return Err(DeepseekError::ConfigError(
                "Max sequence length cannot be greater than max context length".to_string()
            ));
        }
        
        if self.model.default_temperature < 0.0 || self.model.default_temperature > 2.0 {
            return Err(DeepseekError::ConfigError(
                "Temperature must be between 0.0 and 2.0".to_string()
            ));
        }
        
        if self.model.default_top_p < 0.0 || self.model.default_top_p > 1.0 {
            return Err(DeepseekError::ConfigError(
                "Top-p must be between 0.0 and 1.0".to_string()
            ));
        }
        
        if self.model.default_max_tokens == 0 {
            return Err(DeepseekError::ConfigError("Max tokens must be greater than 0".to_string()));
        }
        
        // Validate logging settings
        if !["debug", "info", "warn", "error"].contains(&self.log.level.to_lowercase().as_str()) {
            return Err(DeepseekError::ConfigError(
                "Log level must be one of: debug, info, warn, error".to_string()
            ));
        }
        
        Ok(())
    }
    
    /// Save configuration to a file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let contents = serde_json::to_string_pretty(self)
            .map_err(|e| DeepseekError::ConfigError(format!("Failed to serialize config: {}", e)))?;
            
        std::fs::write(path, contents)
            .map_err(|e| DeepseekError::ConfigError(format!("Failed to write config file: {}", e)))?;
            
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    
    #[test]
    fn test_default_config() {
        let config = Config::default();
        
        assert_eq!(config.api.endpoint, "https://api.deepseek.com/v1");
        assert_eq!(config.model.default_model, "deepseek-coder-6.7b-base");
        assert_eq!(config.log.level, "info");
    }
    
    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        
        // Valid config should pass
        assert!(config.validate().is_ok());
        
        // Invalid temperature
        config.model.default_temperature = 3.0;
        assert!(config.validate().is_err());
        
        // Invalid top-p
        config.model.default_temperature = 0.7;
        config.model.default_top_p = 1.5;
        assert!(config.validate().is_err());
        
        // Invalid max sequence length
        config.model.default_top_p = 0.9;
        config.model.max_sequence_length = 10000;
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_config_file_io() {
        let config = Config::default();
        let temp_file = NamedTempFile::new().unwrap();
        
        // Save config
        config.save_to_file(temp_file.path()).unwrap();
        
        // Load config
        let loaded = Config::load_from_file(temp_file.path()).unwrap();
        
        // Compare
        assert_eq!(config.api.endpoint, loaded.api.endpoint);
        assert_eq!(config.model.default_model, loaded.model.default_model);
        assert_eq!(config.log.level, loaded.log.level);
    }
    
    #[test]
    fn test_env_override() {
        let mut config = Config::default();
        
        // Set environment variables
        env::set_var("DEEPSEEK_API_ENDPOINT", "https://test.api.deepseek.com");
        env::set_var("DEEPSEEK_DEFAULT_MODEL", "test-model");
        env::set_var("DEEPSEEK_LOG_LEVEL", "debug");
        
        // Load from env
        config.load_from_env().unwrap();
        
        // Verify overrides
        assert_eq!(config.api.endpoint, "https://test.api.deepseek.com");
        assert_eq!(config.model.default_model, "test-model");
        assert_eq!(config.log.level, "debug");
    }
} 