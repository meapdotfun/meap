mod types;
pub use types::*;

use crate::error::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait]
pub trait MessageHandler: Send + Sync {
    async fn handle_message(&self, message: Message) -> Result<Option<Message>>;
}

#[async_trait]
pub trait Protocol: Send + Sync {
    /// Validates an incoming message
    async fn validate_message(&self, message: &Message) -> Result<()>;

    /// Processes an incoming message
    async fn process_message(&self, message: Message) -> Result<Option<Message>>;

    /// Sends a message
    async fn send_message(&self, message: Message) -> Result<()>;

    /// Handles stream messages
    async fn handle_stream(&self, message: Message) -> Result<()>;
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
