//! Prompt template management for Deepseek models
//! Provides template handling and variable substitution

use crate::error::{DeepseekError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// A template for generating model prompts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    /// Unique identifier
    pub id: String,
    /// Template description
    pub description: String,
    /// Template content with {{variable}} placeholders
    pub template: String,
    /// Default variables
    #[serde(default)]
    pub defaults: HashMap<String, String>,
    /// Model that this template is optimized for
    pub model: Option<String>,
    /// Token estimation
    pub estimated_tokens: Option<usize>,
}

impl PromptTemplate {
    /// Creates a new prompt template
    pub fn new(id: String, description: String, template: String) -> Self {
        Self {
            id,
            description,
            template,
            defaults: HashMap::new(),
            model: None,
            estimated_tokens: None,
        }
    }

    /// Renders the template with the given variables
    pub fn render(&self, variables: &HashMap<String, String>) -> String {
        let mut result = self.template.clone();
        let mut all_vars = self.defaults.clone();
        
        // Override defaults with provided variables
        for (key, value) in variables {
            all_vars.insert(key.clone(), value.clone());
        }
        
        // Replace variables in template
        for (key, value) in all_vars {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, &value);
        }
        
        result
    }

    /// Estimates tokens for the rendered template
    pub fn estimate_tokens(&self, variables: &HashMap<String, String>) -> usize {
        let rendered = self.render(variables);
        // Very rough estimate: ~4 characters per token for English text
        rendered.len() / 4
    }
}

/// Manages a collection of prompt templates
pub struct PromptManager {
    templates: Arc<RwLock<HashMap<String, PromptTemplate>>>,
}

impl PromptManager {
    /// Creates a new prompt manager
    pub fn new() -> Self {
        Self {
            templates: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Adds a template to the manager
    pub async fn add_template(&self, template: PromptTemplate) -> Result<()> {
        let mut templates = self.templates.write().await;
        templates.insert(template.id.clone(), template);
        Ok(())
    }

    /// Gets a template by ID
    pub async fn get_template(&self, id: &str) -> Result<PromptTemplate> {
        let templates = self.templates.read().await;
        templates.get(id)
            .cloned()
            .ok_or_else(|| DeepseekError::InvalidRequest(format!("Template not found: {}", id)))
    }

    /// Renders a template by ID with variables
    pub async fn render(&self, id: &str, variables: &HashMap<String, String>) -> Result<String> {
        let template = self.get_template(id).await?;
        Ok(template.render(variables))
    }

    /// Loads templates from a JSON file
    pub async fn load_from_file(&self, path: &str) -> Result<usize> {
        let file_content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| DeepseekError::ParseError(format!("Failed to read template file: {}", e)))?;
        
        let templates: Vec<PromptTemplate> = serde_json::from_str(&file_content)
            .map_err(|e| DeepseekError::ParseError(format!("Failed to parse templates: {}", e)))?;
        
        let mut count = 0;
        for template in templates {
            self.add_template(template).await?;
            count += 1;
        }
        
        info!("Loaded {} templates from {}", count, path);
        Ok(count)
    }

    /// Returns all template IDs
    pub async fn list_templates(&self) -> Vec<String> {
        let templates = self.templates.read().await;
        templates.keys().cloned().collect()
    }

    /// Removes a template by ID
    pub async fn remove_template(&self, id: &str) -> Result<()> {
        let mut templates = self.templates.write().await;
        if templates.remove(id).is_none() {
            return Err(DeepseekError::InvalidRequest(format!("Template not found: {}", id)));
        }
        Ok(())
    }
}

/// System prompt builder with predefined sections
pub struct SystemPromptBuilder {
    sections: Vec<(String, String)>,
}

impl SystemPromptBuilder {
    /// Creates a new system prompt builder
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
        }
    }

    /// Adds a role definition section
    pub fn with_role(mut self, role: &str) -> Self {
        self.sections.push(("Role".to_string(), role.to_string()));
        self
    }

    /// Adds a context section
    pub fn with_context(mut self, context: &str) -> Self {
        self.sections.push(("Context".to_string(), context.to_string()));
        self
    }

    /// Adds a guidelines section
    pub fn with_guidelines(mut self, guidelines: &str) -> Self {
        self.sections.push(("Guidelines".to_string(), guidelines.to_string()));
        self
    }

    /// Adds a constraints section
    pub fn with_constraints(mut self, constraints: &str) -> Self {
        self.sections.push(("Constraints".to_string(), constraints.to_string()));
        self
    }

    /// Adds a custom section
    pub fn with_section(mut self, title: &str, content: &str) -> Self {
        self.sections.push((title.to_string(), content.to_string()));
        self
    }

    /// Builds the complete system prompt
    pub fn build(self) -> String {
        let mut result = String::new();
        
        for (title, content) in self.sections {
            result.push_str(&format!("# {}\n{}\n\n", title, content));
        }
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_template_rendering() {
        let template = PromptTemplate::new(
            "test".to_string(),
            "Test template".to_string(),
            "Hello {{name}}, welcome to {{service}}!".to_string(),
        );
        
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "Alice".to_string());
        variables.insert("service".to_string(), "MEAP".to_string());
        
        let rendered = template.render(&variables);
        assert_eq!(rendered, "Hello Alice, welcome to MEAP!");
    }

    #[test]
    fn test_prompt_with_defaults() {
        let mut template = PromptTemplate::new(
            "test".to_string(),
            "Test template".to_string(),
            "Hello {{name}}, welcome to {{service}}!".to_string(),
        );
        
        template.defaults.insert("name".to_string(), "User".to_string());
        template.defaults.insert("service".to_string(), "MEAP".to_string());
        
        // With empty variables, should use defaults
        let rendered = template.render(&HashMap::new());
        assert_eq!(rendered, "Hello User, welcome to MEAP!");
        
        // With provided variables, should override defaults
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "Alice".to_string());
        
        let rendered = template.render(&variables);
        assert_eq!(rendered, "Hello Alice, welcome to MEAP!");
    }

    #[test]
    fn test_system_prompt_builder() {
        let prompt = SystemPromptBuilder::new()
            .with_role("You are a helpful coding assistant")
            .with_guidelines("Provide clear explanations")
            .with_constraints("Don't write full applications")
            .build();
        
        assert!(prompt.contains("# Role\nYou are a helpful coding assistant"));
        assert!(prompt.contains("# Guidelines\nProvide clear explanations"));
        assert!(prompt.contains("# Constraints\nDon't write full applications"));
    }

    #[tokio::test]
    async fn test_prompt_manager() {
        let manager = PromptManager::new();
        
        let template = PromptTemplate::new(
            "greeting".to_string(),
            "Simple greeting".to_string(),
            "Hello {{name}}!".to_string(),
        );
        
        manager.add_template(template).await.unwrap();
        
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "World".to_string());
        
        let rendered = manager.render("greeting", &variables).await.unwrap();
        assert_eq!(rendered, "Hello World!");
        
        let templates = manager.list_templates().await;
        assert_eq!(templates, vec!["greeting"]);
        
        manager.remove_template("greeting").await.unwrap();
        let templates = manager.list_templates().await;
        assert!(templates.is_empty());
    }
} 