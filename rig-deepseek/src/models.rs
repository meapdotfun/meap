//! Model registry and versioning for Deepseek models
//! Manages model metadata, capabilities, and version tracking

use crate::error::{DeepseekError, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Model capabilities
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModelCapability {
    /// Text generation
    TextGeneration,
    /// Code generation
    CodeGeneration,
    /// Chat completion
    ChatCompletion,
    /// Function calling
    FunctionCalling,
    /// Embeddings generation
    Embeddings,
    /// Image generation
    ImageGeneration,
    /// Image understanding
    ImageUnderstanding,
    /// Audio transcription
    AudioTranscription,
    /// Custom capability
    Custom(String),
}

/// Model size category
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelSize {
    /// Small models (1-3B parameters)
    Small,
    /// Medium models (3-13B parameters)
    Medium,
    /// Large models (13-70B parameters)
    Large,
    /// Extra large models (70B+ parameters)
    ExtraLarge,
}

/// Model version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVersion {
    /// Version string (e.g., "1.0.0")
    pub version: String,
    /// Release date
    pub release_date: SystemTime,
    /// Release notes
    pub release_notes: String,
    /// Is this version deprecated
    pub deprecated: bool,
    /// End of life date (if any)
    pub eol_date: Option<SystemTime>,
}

/// Model metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    /// Model ID
    pub id: String,
    /// Display name
    pub name: String,
    /// Model description
    pub description: String,
    /// Model provider
    pub provider: String,
    /// Model family
    pub family: String,
    /// Model size
    pub size: ModelSize,
    /// Parameter count
    pub parameters: u64,
    /// Context window size
    pub context_window: usize,
    /// Model capabilities
    pub capabilities: Vec<ModelCapability>,
    /// Model versions
    pub versions: Vec<ModelVersion>,
    /// Default version
    pub default_version: String,
    /// Model endpoint
    pub endpoint: String,
    /// Pricing information (per 1M tokens)
    pub pricing: ModelPricing,
    /// Is this model available
    pub available: bool,
    /// Custom metadata
    pub custom_metadata: HashMap<String, String>,
}

/// Model pricing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    /// Input token price per 1M tokens
    pub input_price_per_million: f64,
    /// Output token price per 1M tokens
    pub output_price_per_million: f64,
    /// Currency code
    pub currency: String,
}

/// Model registry for managing available models
pub struct ModelRegistry {
    /// Available models
    models: Arc<RwLock<HashMap<String, ModelMetadata>>>,
    /// Default model ID
    default_model: String,
    /// Last refresh time
    last_refresh: Arc<RwLock<SystemTime>>,
}

impl ModelRegistry {
    /// Create a new model registry
    pub fn new(default_model: String) -> Self {
        Self {
            models: Arc::new(RwLock::new(HashMap::new())),
            default_model,
            last_refresh: Arc::new(RwLock::new(SystemTime::now())),
        }
    }
    
    /// Register a model
    pub async fn register_model(&self, model: ModelMetadata) -> Result<()> {
        let mut models = self.models.write().await;
        
        // Validate model before registering
        if model.versions.is_empty() {
            return Err(DeepseekError::InvalidRequest("Model must have at least one version".to_string()));
        }
        
        // Check if default version exists in versions
        if !model.versions.iter().any(|v| v.version == model.default_version) {
            return Err(DeepseekError::InvalidRequest(
                format!("Default version {} not found in model versions", model.default_version)
            ));
        }
        
        models.insert(model.id.clone(), model);
        *self.last_refresh.write().await = SystemTime::now();
        
        Ok(())
    }
    
    /// Get a model by ID
    pub async fn get_model(&self, id: &str) -> Result<ModelMetadata> {
        let models = self.models.read().await;
        
        models.get(id)
            .cloned()
            .ok_or_else(|| DeepseekError::InvalidRequest(format!("Model not found: {}", id)))
    }
    
    /// Get the default model
    pub async fn get_default_model(&self) -> Result<ModelMetadata> {
        self.get_model(&self.default_model).await
    }
    
    /// List all available models
    pub async fn list_models(&self) -> Vec<ModelMetadata> {
        let models = self.models.read().await;
        models.values().cloned().collect()
    }
    
    /// List models with specific capability
    pub async fn list_models_with_capability(&self, capability: ModelCapability) -> Vec<ModelMetadata> {
        let models = self.models.read().await;
        
        models.values()
            .filter(|model| model.capabilities.contains(&capability) && model.available)
            .cloned()
            .collect()
    }
    
    /// Update model availability
    pub async fn update_availability(&self, id: &str, available: bool) -> Result<()> {
        let mut models = self.models.write().await;
        
        if let Some(model) = models.get_mut(id) {
            model.available = available;
            *self.last_refresh.write().await = SystemTime::now();
            Ok(())
        } else {
            Err(DeepseekError::InvalidRequest(format!("Model not found: {}", id)))
        }
    }
    
    /// Add a new version to a model
    pub async fn add_model_version(&self, id: &str, version: ModelVersion) -> Result<()> {
        let mut models = self.models.write().await;
        
        if let Some(model) = models.get_mut(id) {
            // Check if version already exists
            if model.versions.iter().any(|v| v.version == version.version) {
                return Err(DeepseekError::InvalidRequest(
                    format!("Version {} already exists for model {}", version.version, id)
                ));
            }
            
            model.versions.push(version);
            *self.last_refresh.write().await = SystemTime::now();
            Ok(())
        } else {
            Err(DeepseekError::InvalidRequest(format!("Model not found: {}", id)))
        }
    }
    
    /// Set the default version for a model
    pub async fn set_default_version(&self, id: &str, version: &str) -> Result<()> {
        let mut models = self.models.write().await;
        
        if let Some(model) = models.get_mut(id) {
            // Check if version exists
            if !model.versions.iter().any(|v| v.version == version) {
                return Err(DeepseekError::InvalidRequest(
                    format!("Version {} not found for model {}", version, id)
                ));
            }
            
            model.default_version = version.to_string();
            *self.last_refresh.write().await = SystemTime::now();
            Ok(())
        } else {
            Err(DeepseekError::InvalidRequest(format!("Model not found: {}", id)))
        }
    }
    
    /// Mark a model version as deprecated
    pub async fn deprecate_version(&self, id: &str, version: &str, eol_date: Option<SystemTime>) -> Result<()> {
        let mut models = self.models.write().await;
        
        if let Some(model) = models.get_mut(id) {
            // Find and update the version
            let version_found = model.versions.iter_mut().find(|v| v.version == version);
            
            if let Some(v) = version_found {
                v.deprecated = true;
                v.eol_date = eol_date;
                
                // If this was the default version, warn about it
                if model.default_version == version {
                    warn!("Default version {} for model {} is now deprecated", version, id);
                }
                
                *self.last_refresh.write().await = SystemTime::now();
                Ok(())
            } else {
                Err(DeepseekError::InvalidRequest(
                    format!("Version {} not found for model {}", version, id)
                ))
            }
        } else {
            Err(DeepseekError::InvalidRequest(format!("Model not found: {}", id)))
        }
    }
    
    /// Get time since last registry update
    pub async fn time_since_refresh(&self) -> Duration {
        let last_refresh = *self.last_refresh.read().await;
        SystemTime::now().duration_since(last_refresh).unwrap_or(Duration::from_secs(0))
    }
    
    /// Load models from a JSON file
    pub async fn load_from_file(&self, path: &str) -> Result<usize> {
        let file_content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| DeepseekError::ParseError(format!("Failed to read models file: {}", e)))?;
        
        let models: Vec<ModelMetadata> = serde_json::from_str(&file_content)
            .map_err(|e| DeepseekError::ParseError(format!("Failed to parse models: {}", e)))?;
        
        let mut count = 0;
        for model in models {
            self.register_model(model).await?;
            count += 1;
        }
        
        info!("Loaded {} models from {}", count, path);
        Ok(count)
    }
    
    /// Save models to a JSON file
    pub async fn save_to_file(&self, path: &str) -> Result<()> {
        let models = self.list_models().await;
        
        let json = serde_json::to_string_pretty(&models)
            .map_err(|e| DeepseekError::ParseError(format!("Failed to serialize models: {}", e)))?;
        
        tokio::fs::write(path, json)
            .await
            .map_err(|e| DeepseekError::ParseError(format!("Failed to write models file: {}", e)))?;
        
        Ok(())
    }
    
    /// Initialize with default Deepseek models
    pub async fn initialize_defaults(&self) -> Result<()> {
        // Deepseek Coder models
        let deepseek_coder = ModelMetadata {
            id: "deepseek-coder-6.7b-base".to_string(),
            name: "Deepseek Coder 6.7B Base".to_string(),
            description: "Base model for code generation and understanding".to_string(),
            provider: "Deepseek".to_string(),
            family: "Deepseek Coder".to_string(),
            size: ModelSize::Medium,
            parameters: 6_700_000_000,
            context_window: 16384,
            capabilities: vec![
                ModelCapability::CodeGeneration,
                ModelCapability::TextGeneration,
            ],
            versions: vec![
                ModelVersion {
                    version: "1.0.0".to_string(),
                    release_date: SystemTime::now(),
                    release_notes: "Initial release".to_string(),
                    deprecated: false,
                    eol_date: None,
                }
            ],
            default_version: "1.0.0".to_string(),
            endpoint: "/v1/completions".to_string(),
            pricing: ModelPricing {
                input_price_per_million: 0.5,
                output_price_per_million: 1.5,
                currency: "USD".to_string(),
            },
            available: true,
            custom_metadata: HashMap::new(),
        };
        
        let deepseek_coder_instruct = ModelMetadata {
            id: "deepseek-coder-6.7b-instruct".to_string(),
            name: "Deepseek Coder 6.7B Instruct".to_string(),
            description: "Instruction-tuned model for code generation and understanding".to_string(),
            provider: "Deepseek".to_string(),
            family: "Deepseek Coder".to_string(),
            size: ModelSize::Medium,
            parameters: 6_700_000_000,
            context_window: 16384,
            capabilities: vec![
                ModelCapability::CodeGeneration,
                ModelCapability::TextGeneration,
                ModelCapability::ChatCompletion,
            ],
            versions: vec![
                ModelVersion {
                    version: "1.0.0".to_string(),
                    release_date: SystemTime::now(),
                    release_notes: "Initial release".to_string(),
                    deprecated: false,
                    eol_date: None,
                }
            ],
            default_version: "1.0.0".to_string(),
            endpoint: "/v1/chat/completions".to_string(),
            pricing: ModelPricing {
                input_price_per_million: 0.5,
                output_price_per_million: 1.5,
                currency: "USD".to_string(),
            },
            available: true,
            custom_metadata: HashMap::new(),
        };
        
        let deepseek_coder_33b = ModelMetadata {
            id: "deepseek-coder-33b-instruct".to_string(),
            name: "Deepseek Coder 33B Instruct".to_string(),
            description: "Large instruction-tuned model for code generation and understanding".to_string(),
            provider: "Deepseek".to_string(),
            family: "Deepseek Coder".to_string(),
            size: ModelSize::Large,
            parameters: 33_000_000_000,
            context_window: 16384,
            capabilities: vec![
                ModelCapability::CodeGeneration,
                ModelCapability::TextGeneration,
                ModelCapability::ChatCompletion,
                ModelCapability::FunctionCalling,
            ],
            versions: vec![
                ModelVersion {
                    version: "1.0.0".to_string(),
                    release_date: SystemTime::now(),
                    release_notes: "Initial release".to_string(),
                    deprecated: false,
                    eol_date: None,
                }
            ],
            default_version: "1.0.0".to_string(),
            endpoint: "/v1/chat/completions".to_string(),
            pricing: ModelPricing {
                input_price_per_million: 1.5,
                output_price_per_million: 5.0,
                currency: "USD".to_string(),
            },
            available: true,
            custom_metadata: HashMap::new(),
        };
        
        // Deepseek LLM models
        let deepseek_llm = ModelMetadata {
            id: "deepseek-llm-7b-base".to_string(),
            name: "Deepseek LLM 7B Base".to_string(),
            description: "Base language model for general text generation".to_string(),
            provider: "Deepseek".to_string(),
            family: "Deepseek LLM".to_string(),
            size: ModelSize::Medium,
            parameters: 7_000_000_000,
            context_window: 4096,
            capabilities: vec![
                ModelCapability::TextGeneration,
            ],
            versions: vec![
                ModelVersion {
                    version: "1.0.0".to_string(),
                    release_date: SystemTime::now(),
                    release_notes: "Initial release".to_string(),
                    deprecated: false,
                    eol_date: None,
                }
            ],
            default_version: "1.0.0".to_string(),
            endpoint: "/v1/completions".to_string(),
            pricing: ModelPricing {
                input_price_per_million: 0.2,
                output_price_per_million: 0.8,
                currency: "USD".to_string(),
            },
            available: true,
            custom_metadata: HashMap::new(),
        };
        
        let deepseek_llm_chat = ModelMetadata {
            id: "deepseek-llm-67b-chat".to_string(),
            name: "Deepseek LLM 67B Chat".to_string(),
            description: "Large language model optimized for chat and conversation".to_string(),
            provider: "Deepseek".to_string(),
            family: "Deepseek LLM".to_string(),
            size: ModelSize::Large,
            parameters: 67_000_000_000,
            context_window: 4096,
            capabilities: vec![
                ModelCapability::TextGeneration,
                ModelCapability::ChatCompletion,
                ModelCapability::FunctionCalling,
            ],
            versions: vec![
                ModelVersion {
                    version: "1.0.0".to_string(),
                    release_date: SystemTime::now(),
                    release_notes: "Initial release".to_string(),
                    deprecated: false,
                    eol_date: None,
                }
            ],
            default_version: "1.0.0".to_string(),
            endpoint: "/v1/chat/completions".to_string(),
            pricing: ModelPricing {
                input_price_per_million: 1.0,
                output_price_per_million: 3.0,
                currency: "USD".to_string(),
            },
            available: true,
            custom_metadata: HashMap::new(),
        };
        
        // Register all models
        self.register_model(deepseek_coder).await?;
        self.register_model(deepseek_coder_instruct).await?;
        self.register_model(deepseek_coder_33b).await?;
        self.register_model(deepseek_llm).await?;
        self.register_model(deepseek_llm_chat).await?;
        
        info!("Initialized default Deepseek models");
        Ok(())
    }
}

/// Model selection helper
pub struct ModelSelector {
    registry: Arc<ModelRegistry>,
}

impl ModelSelector {
    /// Create a new model selector
    pub fn new(registry: Arc<ModelRegistry>) -> Self {
        Self { registry }
    }
    
    /// Select best model for code generation
    pub async fn select_for_code(&self, context_size: usize) -> Result<String> {
        let models = self.registry.list_models_with_capability(ModelCapability::CodeGeneration).await;
        
        // Filter by context size
        let suitable_models: Vec<_> = models.into_iter()
            .filter(|m| m.context_window >= context_size)
            .collect();
        
        if suitable_models.is_empty() {
            return Err(DeepseekError::InvalidRequest(
                format!("No suitable model found for context size {}", context_size)
            ));
        }
        
        // Select smallest model that can handle the context
        let selected = suitable_models.iter()
            .min_by_key(|m| m.parameters)
            .unwrap();
        
        Ok(selected.id.clone())
    }
    
    /// Select best model for chat
    pub async fn select_for_chat(&self, context_size: usize, need_function_calling: bool) -> Result<String> {
        let models = self.registry.list_models_with_capability(ModelCapability::ChatCompletion).await;
        
        // Filter by context size and function calling if needed
        let suitable_models: Vec<_> = models.into_iter()
            .filter(|m| m.context_window >= context_size)
            .filter(|m| !need_function_calling || m.capabilities.contains(&ModelCapability::FunctionCalling))
            .collect();
        
        if suitable_models.is_empty() {
            return Err(DeepseekError::InvalidRequest(
                format!("No suitable model found for chat with context size {}", context_size)
            ));
        }
        
        // Select smallest model that can handle the requirements
        let selected = suitable_models.iter()
            .min_by_key(|m| m.parameters)
            .unwrap();
        
        Ok(selected.id.clone())
    }
    
    /// Calculate cost for a request
    pub async fn calculate_cost(&self, model_id: &str, input_tokens: usize, output_tokens: usize) -> Result<f64> {
        let model = self.registry.get_model(model_id).await?;
        
        let input_cost = (input_tokens as f64 / 1_000_000.0) * model.pricing.input_price_per_million;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * model.pricing.output_price_per_million;
        
        Ok(input_cost + output_cost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_model_registry_basic() {
        let registry = ModelRegistry::new("test-model".to_string());
        
        // Create a test model
        let model = ModelMetadata {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            description: "A test model".to_string(),
            provider: "Test".to_string(),
            family: "Test Family".to_string(),
            size: ModelSize::Small,
            parameters: 1_000_000_000,
            context_window: 4096,
            capabilities: vec![
                ModelCapability::TextGeneration,
                ModelCapability::CodeGeneration,
            ],
            versions: vec![
                ModelVersion {
                    version: "1.0.0".to_string(),
                    release_date: SystemTime::now(),
                    release_notes: "Initial release".to_string(),
                    deprecated: false,
                    eol_date: None,
                }
            ],
            default_version: "1.0.0".to_string(),
            endpoint: "/v1/completions".to_string(),
            pricing: ModelPricing {
                input_price_per_million: 0.5,
                output_price_per_million: 1.5,
                currency: "USD".to_string(),
            },
            available: true,
            custom_metadata: HashMap::new(),
        };
        
        // Register the model
        registry.register_model(model.clone()).await.unwrap();
        
        // Get the model
        let retrieved = registry.get_model("test-model").await.unwrap();
        assert_eq!(retrieved.id, "test-model");
        assert_eq!(retrieved.name, "Test Model");
        
        // List models
        let models = registry.list_models().await;
        assert_eq!(models.len(), 1);
        
        // List models with capability
        let code_models = registry.list_models_with_capability(ModelCapability::CodeGeneration).await;
        assert_eq!(code_models.len(), 1);
        
        let image_models = registry.list_models_with_capability(ModelCapability::ImageGeneration).await;
        assert_eq!(image_models.len(), 0);
    }
    
    #[tokio::test]
    async fn test_model_versioning() {
        let registry = ModelRegistry::new("test-model".to_string());
        
        // Create a test model
        let model = ModelMetadata {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            description: "A test model".to_string(),
            provider: "Test".to_string(),
            family: "Test Family".to_string(),
            size: ModelSize::Small,
            parameters: 1_000_000_000,
            context_window: 4096,
            capabilities: vec![
                ModelCapability::TextGeneration,
            ],
            versions: vec![
                ModelVersion {
                    version: "1.0.0".to_string(),
                    release_date: SystemTime::now(),
                    release_notes: "Initial release".to_string(),
                    deprecated: false,
                    eol_date: None,
                }
            ],
            default_version: "1.0.0".to_string(),
            endpoint: "/v1/completions".to_string(),
            pricing: ModelPricing {
                input_price_per_million: 0.5,
                output_price_per_million: 1.5,
                currency: "USD".to_string(),
            },
            available: true,
            custom_metadata: HashMap::new(),
        };
        
        // Register the model
        registry.register_model(model).await.unwrap();
        
        // Add a new version
        let new_version = ModelVersion {
            version: "1.1.0".to_string(),
            release_date: SystemTime::now(),
            release_notes: "Bug fixes".to_string(),
            deprecated: false,
            eol_date: None,
        };
        
        registry.add_model_version("test-model", new_version).await.unwrap();
        
        // Set as default
        registry.set_default_version("test-model", "1.1.0").await.unwrap();
        
        // Check default version
        let model = registry.get_model("test-model").await.unwrap();
        assert_eq!(model.default_version, "1.1.0");
        assert_eq!(model.versions.len(), 2);
        
        // Deprecate version
        registry.deprecate_version("test-model", "1.0.0", None).await.unwrap();
        
        // Check deprecation
        let model = registry.get_model("test-model").await.unwrap();
        let deprecated_version = model.versions.iter().find(|v| v.version == "1.0.0").unwrap();
        assert!(deprecated_version.deprecated);
    }
    
    #[tokio::test]
    async fn test_model_selector() {
        let registry = Arc::new(ModelRegistry::new("test-model".to_string()));
        let selector = ModelSelector::new(registry.clone());
        
        // Initialize with test models
        let small_model = ModelMetadata {
            id: "small-code-model".to_string(),
            name: "Small Code Model".to_string(),
            description: "A small code model".to_string(),
            provider: "Test".to_string(),
            family: "Test Family".to_string(),
            size: ModelSize::Small,
            parameters: 1_000_000_000,
            context_window: 4096,
            capabilities: vec![
                ModelCapability::CodeGeneration,
                ModelCapability::ChatCompletion,
            ],
            versions: vec![
                ModelVersion {
                    version: "1.0.0".to_string(),
                    release_date: SystemTime::now(),
                    release_notes: "Initial release".to_string(),
                    deprecated: false,
                    eol_date: None,
                }
            ],
            default_version: "1.0.0".to_string(),
            endpoint: "/v1/completions".to_string(),
            pricing: ModelPricing {
                input_price_per_million: 0.5,
                output_price_per_million: 1.5,
                currency: "USD".to_string(),
            },
            available: true,
            custom_metadata: HashMap::new(),
        };
        
        let large_model = ModelMetadata {
            id: "large-code-model".to_string(),
            name: "Large Code Model".to_string(),
            description: "A large code model".to_string(),
            provider: "Test".to_string(),
            family: "Test Family".to_string(),
            size: ModelSize::Large,
            parameters: 30_000_000_000,
            context_window: 16384,
            capabilities: vec![
                ModelCapability::CodeGeneration,
                ModelCapability::ChatCompletion,
                ModelCapability::FunctionCalling,
            ],
            versions: vec![
                ModelVersion {
                    version: "1.0.0".to_string(),
                    release_date: SystemTime::now(),
                    release_notes: "Initial release".to_string(),
                    deprecated: false,
                    eol_date: None,
                }
            ],
            default_version: "1.0.0".to_string(),
            endpoint: "/v1/completions".to_string(),
            pricing: ModelPricing {
                input_price_per_million: 2.0,
                output_price_per_million: 6.0,
                currency: "USD".to_string(),
            },
            available: true,
            custom_metadata: HashMap::new(),
        };
        
        registry.register_model(small_model).await.unwrap();
        registry.register_model(large_model).await.unwrap();
        
        // Test model selection for code
        let small_context_model = selector.select_for_code(2000).await.unwrap();
        assert_eq!(small_context_model, "small-code-model");
        
        let large_context_model = selector.select_for_code(8000).await.unwrap();
        assert_eq!(large_context_model, "large-code-model");
        
        // Test model selection for chat with function calling
        let function_model = selector.select_for_chat(2000, true).await.unwrap();
        assert_eq!(function_model, "large-code-model");
        
        // Test cost calculation
        let cost = selector.calculate_cost("small-code-model", 1000, 500).await.unwrap();
        let expected_cost = (1000.0 / 1_000_000.0) * 0.5 + (500.0 / 1_000_000.0) * 1.5;
        assert!((cost - expected_cost).abs() < 0.0001);
    }
} 