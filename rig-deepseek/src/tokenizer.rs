//! Tokenizer for Deepseek models
//! Handles token counting, text splitting, and token manipulation

use crate::error::{DeepseekError, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Token information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    /// Token ID in vocabulary
    pub id: u32,
    /// Text representation
    pub text: String,
    /// Token type
    pub token_type: TokenType,
    /// Token score/probability
    pub score: f32,
}

/// Types of tokens
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenType {
    /// Regular text token
    Text,
    /// Special token like BOS, EOS, PAD
    Special,
    /// Whitespace token
    Whitespace,
    /// Punctuation token
    Punctuation,
    /// Number token
    Number,
}

/// Configuration for tokenizer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenizerConfig {
    /// Path to vocabulary file
    pub vocab_path: String,
    /// Maximum sequence length
    pub max_length: usize,
    /// Whether to add special tokens
    pub add_special_tokens: bool,
    /// Truncation strategy
    pub truncation_strategy: TruncationStrategy,
    /// Padding strategy
    pub padding_strategy: PaddingStrategy,
}

/// Truncation strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TruncationStrategy {
    /// Truncate from the end
    TruncateEnd,
    /// Truncate from the start
    TruncateStart,
    /// Truncate from both ends
    TruncateMiddle,
}

/// Padding strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaddingStrategy {
    /// No padding
    NoPadding,
    /// Pad to longest sequence
    LongestSequence,
    /// Pad to max length
    MaxLength,
}

/// Tokenizer for processing text
pub struct Tokenizer {
    /// Configuration
    config: TokenizerConfig,
    /// Vocabulary (token text -> token id)
    vocab: Arc<RwLock<HashMap<String, u32>>>,
    /// Reverse vocabulary (token id -> token text)
    reverse_vocab: Arc<RwLock<HashMap<u32, String>>>,
    /// Special tokens
    special_tokens: Arc<RwLock<HashMap<String, Token>>>,
}

impl Tokenizer {
    /// Create a new tokenizer
    pub async fn new(config: TokenizerConfig) -> Result<Self> {
        let vocab = Arc::new(RwLock::new(HashMap::new()));
        let reverse_vocab = Arc::new(RwLock::new(HashMap::new()));
        let special_tokens = Arc::new(RwLock::new(HashMap::new()));
        
        let tokenizer = Self {
            config,
            vocab,
            reverse_vocab,
            special_tokens,
        };
        
        tokenizer.load_vocabulary().await?;
        tokenizer.initialize_special_tokens().await?;
        
        Ok(tokenizer)
    }
    
    /// Load vocabulary from file
    async fn load_vocabulary(&self) -> Result<()> {
        let contents = tokio::fs::read_to_string(&self.config.vocab_path)
            .await
            .map_err(|e| DeepseekError::TokenizerError(format!("Failed to read vocab file: {}", e)))?;
            
        let mut vocab = self.vocab.write().await;
        let mut reverse_vocab = self.reverse_vocab.write().await;
        
        for (i, line) in contents.lines().enumerate() {
            let token_text = line.trim().to_string();
            let token_id = i as u32;
            
            vocab.insert(token_text.clone(), token_id);
            reverse_vocab.insert(token_id, token_text);
        }
        
        debug!("Loaded {} tokens from vocabulary", vocab.len());
        Ok(())
    }
    
    /// Initialize special tokens
    async fn initialize_special_tokens(&self) -> Result<()> {
        let mut special_tokens = self.special_tokens.write().await;
        
        // Add standard special tokens
        let special_token_texts = vec![
            ("<s>", 0),      // Beginning of sequence
            ("</s>", 1),     // End of sequence
            ("<pad>", 2),    // Padding
            ("<unk>", 3),    // Unknown token
            ("<mask>", 4),   // Mask token
        ];
        
        for (text, id) in special_token_texts {
            special_tokens.insert(text.to_string(), Token {
                id: id,
                text: text.to_string(),
                token_type: TokenType::Special,
                score: 1.0,
            });
        }
        
        Ok(())
    }
    
    /// Encode text into tokens
    pub async fn encode(&self, text: &str) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        let vocab = self.vocab.read().await;
        
        // Add BOS token if configured
        if self.config.add_special_tokens {
            if let Some(bos_token) = self.special_tokens.read().await.get("<s>") {
                tokens.push(bos_token.clone());
            }
        }
        
        // Simple whitespace tokenization for demonstration
        // In a real implementation, this would use proper subword tokenization
        for word in text.split_whitespace() {
            if let Some(&token_id) = vocab.get(word) {
                tokens.push(Token {
                    id: token_id,
                    text: word.to_string(),
                    token_type: TokenType::Text,
                    score: 1.0,
                });
            } else {
                // Handle unknown tokens
                if let Some(unk_token) = self.special_tokens.read().await.get("<unk>") {
                    tokens.push(unk_token.clone());
                }
            }
        }
        
        // Add EOS token if configured
        if self.config.add_special_tokens {
            if let Some(eos_token) = self.special_tokens.read().await.get("</s>") {
                tokens.push(eos_token.clone());
            }
        }
        
        // Apply truncation if needed
        if tokens.len() > self.config.max_length {
            tokens = match self.config.truncation_strategy {
                TruncationStrategy::TruncateEnd => {
                    tokens[..self.config.max_length].to_vec()
                },
                TruncationStrategy::TruncateStart => {
                    tokens[tokens.len() - self.config.max_length..].to_vec()
                },
                TruncationStrategy::TruncateMiddle => {
                    let half_length = self.config.max_length / 2;
                    let mut truncated = tokens[..half_length].to_vec();
                    truncated.extend_from_slice(&tokens[tokens.len() - half_length..]);
                    truncated
                },
            };
        }
        
        // Apply padding if needed
        match self.config.padding_strategy {
            PaddingStrategy::MaxLength => {
                while tokens.len() < self.config.max_length {
                    if let Some(pad_token) = self.special_tokens.read().await.get("<pad>") {
                        tokens.push(pad_token.clone());
                    }
                }
            },
            PaddingStrategy::LongestSequence => {
                // This would be handled at batch level
                warn!("LongestSequence padding requires batch context");
            },
            PaddingStrategy::NoPadding => {},
        }
        
        Ok(tokens)
    }
    
    /// Decode tokens back to text
    pub async fn decode(&self, tokens: &[Token]) -> Result<String> {
        let mut text = String::new();
        let reverse_vocab = self.reverse_vocab.read().await;
        
        for token in tokens {
            if token.token_type == TokenType::Special {
                continue; // Skip special tokens in output
            }
            
            if let Some(token_text) = reverse_vocab.get(&token.id) {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(token_text);
            }
        }
        
        Ok(text)
    }
    
    /// Count tokens in text
    pub async fn count_tokens(&self, text: &str) -> Result<usize> {
        let tokens = self.encode(text).await?;
        Ok(tokens.len())
    }
    
    /// Get token type
    pub fn get_token_type(c: char) -> TokenType {
        if c.is_whitespace() {
            TokenType::Whitespace
        } else if c.is_ascii_punctuation() {
            TokenType::Punctuation
        } else if c.is_ascii_digit() {
            TokenType::Number
        } else {
            TokenType::Text
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;
    
    async fn create_test_vocab() -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        let mut async_file = File::create(file.path()).await.unwrap();
        
        // Write some test vocabulary
        let vocab = "hello\nworld\ntest\ntoken\n";
        async_file.write_all(vocab.as_bytes()).await.unwrap();
        
        file
    }
    
    #[tokio::test]
    async fn test_tokenizer_basic() {
        let vocab_file = create_test_vocab().await;
        
        let config = TokenizerConfig {
            vocab_path: vocab_file.path().to_str().unwrap().to_string(),
            max_length: 10,
            add_special_tokens: true,
            truncation_strategy: TruncationStrategy::TruncateEnd,
            padding_strategy: PaddingStrategy::NoPadding,
        };
        
        let tokenizer = Tokenizer::new(config).await.unwrap();
        
        // Test encoding
        let tokens = tokenizer.encode("hello world").await.unwrap();
        assert!(tokens.len() >= 2); // At least "hello" and "world" tokens
        
        // Test decoding
        let text = tokenizer.decode(&tokens).await.unwrap();
        assert_eq!(text.trim(), "hello world");
    }
    
    #[tokio::test]
    async fn test_tokenizer_truncation() {
        let vocab_file = create_test_vocab().await;
        
        let config = TokenizerConfig {
            vocab_path: vocab_file.path().to_str().unwrap().to_string(),
            max_length: 3,
            add_special_tokens: true,
            truncation_strategy: TruncationStrategy::TruncateEnd,
            padding_strategy: PaddingStrategy::NoPadding,
        };
        
        let tokenizer = Tokenizer::new(config).await.unwrap();
        
        let tokens = tokenizer.encode("hello world test token").await.unwrap();
        assert_eq!(tokens.len(), 3); // Should be truncated to max_length
    }
    
    #[tokio::test]
    async fn test_tokenizer_padding() {
        let vocab_file = create_test_vocab().await;
        
        let config = TokenizerConfig {
            vocab_path: vocab_file.path().to_str().unwrap().to_string(),
            max_length: 5,
            add_special_tokens: true,
            truncation_strategy: TruncationStrategy::TruncateEnd,
            padding_strategy: PaddingStrategy::MaxLength,
        };
        
        let tokenizer = Tokenizer::new(config).await.unwrap();
        
        let tokens = tokenizer.encode("hello").await.unwrap();
        assert_eq!(tokens.len(), 5); // Should be padded to max_length
    }
    
    #[tokio::test]
    async fn test_token_counting() {
        let vocab_file = create_test_vocab().await;
        
        let config = TokenizerConfig {
            vocab_path: vocab_file.path().to_str().unwrap().to_string(),
            max_length: 10,
            add_special_tokens: false,
            truncation_strategy: TruncationStrategy::TruncateEnd,
            padding_strategy: PaddingStrategy::NoPadding,
        };
        
        let tokenizer = Tokenizer::new(config).await.unwrap();
        
        let count = tokenizer.count_tokens("hello world").await.unwrap();
        assert_eq!(count, 2);
    }
} 