use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageType {
    Request,
    Response,
    Error,
    Stream,
    Heartbeat,
    Discovery,
    Registration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub message_type: MessageType,
    pub from: String,
    pub to: String,
    pub content: serde_json::Value,
    pub timestamp: u64,
    pub correlation_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

impl Message {
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

    pub fn with_correlation(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

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
