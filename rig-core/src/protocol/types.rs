//! Protocol types module containing message and type definitions.
//! 
//! This module defines the core message types and structures used
//! throughout the MEAP protocol.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Represents different types of messages in the protocol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageType {
    /// Request messages expect a response
    Request,
    /// Response messages are sent in reply to requests
    Response,
    /// Error messages indicate failures
    Error,
    /// Stream messages are part of a continuous data stream
    Stream,
    /// Heartbeat messages maintain connection status
    Heartbeat,
    /// Discovery messages are used for agent discovery
    Discovery,
    /// Registration messages are used for agent registration
    Registration,
}

/// Core message structure used throughout the protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier
    pub id: String,
    /// Type of message
    pub message_type: MessageType,
    /// Sender agent ID
    pub from: String,
    /// Recipient agent ID
    pub to: String,
    /// Message content as JSON
    pub content: serde_json::Value,
    /// Unix timestamp of message creation
    pub timestamp: u64,
    /// Optional correlation ID for request/response pairs
    pub correlation_id: Option<String>,
    /// Optional metadata as JSON
    pub metadata: Option<serde_json::Value>,
}

impl Message {
    /// Creates a new message with the given parameters.
    /// 
    /// # Arguments
    /// * `message_type` - Type of the message
    /// * `from` - Sender agent ID
    /// * `to` - Recipient agent ID
    /// * `content` - Message content as JSON
    pub fn new(
        message_type: MessageType,
        from: String,
        to: String,
        content: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            message_type,
            from,
            to,
            content,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            correlation_id: None,
            metadata: None,
        }
    }

    /// Adds a correlation ID to the message.
    pub fn with_correlation(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Adds metadata to the message.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::new(
            MessageType::Request,
            "agent1".to_string(),
            "agent2".to_string(),
            serde_json::json!({"test": "data"}),
        );

        assert_eq!(msg.from, "agent1");
        assert_eq!(msg.to, "agent2");
        assert_eq!(msg.message_type, MessageType::Request);
    }
}
