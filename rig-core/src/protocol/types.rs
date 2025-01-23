//! Protocol types module containing message and type definitions.
//! 
//! This module defines the core message types and structures used
//! throughout the MEAP protocol.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use std::fmt;

/// Protocol version information
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl ProtocolVersion {
    pub const CURRENT: ProtocolVersion = ProtocolVersion {
        major: 0,
        minor: 1,
        patch: 0,
    };

    pub fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self { major, minor, patch }
    }

    pub fn is_compatible(&self, other: &ProtocolVersion) -> bool {
        // Major version must match, minor must be >= required
        self.major == other.major && self.minor >= other.minor
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Represents different types of messages in the protocol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Version check messages are used for version compatibility checks
    #[serde(rename = "version_check")]
    VersionCheck,
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
    pub metadata: Option<serde_json::Map<String, serde_json::Value>>,
    /// Protocol version of the message
    pub protocol_version: ProtocolVersion,
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
            protocol_version: ProtocolVersion::CURRENT,
        }
    }

    /// Adds a correlation ID to the message.
    pub fn with_correlation(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Adds metadata to the message.
    pub fn with_metadata(mut self, metadata: serde_json::Map<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Creates a version check response message.
    pub fn version_check_response(&self, compatible: bool) -> Self {
        Self::new(
            MessageType::Response,
            self.to.clone(),
            self.from.clone(),
            serde_json::json!({
                "compatible": compatible,
                "server_version": ProtocolVersion::CURRENT,
            }),
        )
        .with_correlation(self.id.clone())
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
