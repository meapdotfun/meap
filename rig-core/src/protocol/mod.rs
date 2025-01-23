//! Protocol module provides the core messaging and communication functionality.
//! 
//! This module defines the traits and types needed for agent communication,
//! including message handling, validation, and routing.
//! 
//! # Examples
//! 
//! ```rust,no_run
//! use meap_core::{
//!     protocol::{Protocol, Message, MessageType},
//!     error::Result,
//! };
//! use async_trait::async_trait;
//! 
//! struct MyProtocol;
//! 
//! #[async_trait]
//! impl Protocol for MyProtocol {
//!     async fn validate_message(&self, message: &Message) -> Result<()> {
//!         // Implement validation logic
//!         Ok(())
//!     }
//! 
//!     async fn process_message(&self, message: Message) -> Result<Option<Message>> {
//!         // Process the message
//!         Ok(None)
//!     }
//! 
//!     async fn send_message(&self, message: Message) -> Result<()> {
//!         // Send the message
//!         Ok(())
//!     }
//! 
//!     async fn handle_stream(&self, message: Message) -> Result<()> {
//!         // Handle streaming messages
//!         Ok(())
//!     }
//! }
//! ```

mod types;
mod stream;

pub use types::*;
pub use stream::{StreamChunk, StreamControl, StreamSender, StreamReceiver};

use crate::error::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::protocol::types::ProtocolVersion;

#[async_trait]
pub trait MessageHandler: Send + Sync {
    async fn handle_message(&self, message: Message) -> Result<Option<Message>>;
}

pub struct ProtocolHandler {
    handlers: Arc<RwLock<Vec<Box<dyn MessageHandler>>>>,
}

impl ProtocolHandler {
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn add_handler(&self, handler: Box<dyn MessageHandler>) {
        let mut handlers = self.handlers.write().await;
        handlers.push(handler);
    }

    pub async fn process_message(&self, message: Message) -> Result<Vec<Message>> {
        let handlers = self.handlers.read().await;
        let mut responses = Vec::new();

        for handler in handlers.iter() {
            if let Some(response) = handler.handle_message(message.clone()).await? {
                responses.push(response);
            }
        }

        Ok(responses)
    }
}

/// Core protocol trait that must be implemented by all protocol handlers.
#[async_trait]
pub trait Protocol: Send + Sync {
    /// Returns the protocol version supported by this implementation
    fn version(&self) -> ProtocolVersion {
        ProtocolVersion::CURRENT
    }

    /// Checks if a message's protocol version is compatible
    async fn check_version(&self, message: &Message) -> Result<bool> {
        Ok(self.version().is_compatible(&message.protocol_version))
    }

    /// Validates an incoming message before processing.
    /// 
    /// # Arguments
    /// * `message` - The message to validate
    /// 
    /// # Returns
    /// * `Ok(())` if validation succeeds
    /// * `Error` if validation fails
    async fn validate_message(&self, message: &Message) -> Result<()>;

    /// Processes an incoming message and optionally returns a response.
    /// 
    /// # Arguments
    /// * `message` - The message to process
    /// 
    /// # Returns
    /// * `Ok(Some(message))` if a response should be sent
    /// * `Ok(None)` if no response is needed
    /// * `Error` if processing fails
    async fn process_message(&self, message: Message) -> Result<Option<Message>>;

    /// Sends a message to its destination.
    /// 
    /// # Arguments
    /// * `message` - The message to send
    /// 
    /// # Returns
    /// * `Ok(())` if sending succeeds
    /// * `Error` if sending fails
    async fn send_message(&self, message: Message) -> Result<()>;

    /// Handles streaming messages.
    /// 
    /// # Arguments
    /// * `message` - The streaming message to handle
    /// 
    /// # Returns
    /// * `Ok(())` if handling succeeds
    /// * `Error` if handling fails
    async fn handle_stream(&self, message: Message) -> Result<()>;

    /// Initializes a new outgoing stream
    async fn start_stream(&self, metadata: Option<serde_json::Value>) -> Result<StreamSender> {
        let (sender, _) = StreamSender::new(32);
        Ok(sender)
    }

    /// Handles an incoming stream
    async fn handle_stream(&self, control: StreamControl) -> Result<Option<StreamReceiver>> {
        Ok(None)
    }

    /// Processes a stream chunk
    async fn process_chunk(&self, chunk: StreamChunk) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TestHandler;

    #[async_trait]
    impl MessageHandler for TestHandler {
        async fn handle_message(&self, message: Message) -> Result<Option<Message>> {
            Ok(Some(Message::new(
                MessageType::Response,
                "test".to_string(),
                message.from,
                json!({"response": "test"}),
            )))
        }
    }

    #[tokio::test]
    async fn test_protocol_handler() {
        let handler = ProtocolHandler::new();
        handler.add_handler(Box::new(TestHandler)).await;

        let message = Message::new(
            MessageType::Request,
            "sender".to_string(),
            "test".to_string(),
            json!({"test": "data"}),
        );

        let responses = handler.process_message(message).await.unwrap();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].to, "sender");
    }
}
