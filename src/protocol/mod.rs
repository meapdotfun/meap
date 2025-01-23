use crate::{Message, MessageType};
use uuid::Uuid;
use std::time::{SystemTime, UNIX_EPOCH};

pub trait Protocol {
    fn create_message(&self, to: String, content: serde_json::Value, msg_type: MessageType) -> Message;
    fn validate_message(&self, message: &Message) -> bool;
}

pub struct MeapProtocol {
    agent_id: String,
}

impl MeapProtocol {
    pub fn new(agent_id: String) -> Self {
        Self { agent_id }
    }
}

impl Protocol for MeapProtocol {
    fn create_message(&self, to: String, content: serde_json::Value, msg_type: MessageType) -> Message {
        Message {
            id: Uuid::new_v4().to_string(),
            message_type: msg_type,
            from: self.agent_id.clone(),
            to,
            content,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    fn validate_message(&self, message: &Message) -> bool {
        // Basic validation
        if message.from.is_empty() || message.to.is_empty() {
            return false;
        }

        // Validate timestamp
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // Message shouldn't be from the future
        if message.timestamp > now {
            return false;
        }

        true
    }
} 