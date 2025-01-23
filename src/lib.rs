use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MessageType {
    Request,
    Response,
    Error,
    Stream,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub id: String,
    pub message_type: MessageType,
    pub from: String,
    pub to: String,
    pub content: serde_json::Value,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AgentStatus {
    Online,
    Offline,
    Busy,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Agent {
    pub id: String,
    pub capabilities: Vec<String>,
    pub status: AgentStatus,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StreamMessage {
    pub chunk: String,
    pub index: u32,
    pub final_chunk: bool,
} 