//! Agent module provides core agent functionality and types.
//! 
//! This module defines the Agent structure and related types that form the
//! foundation of MEAP's agent-based architecture. Agents are autonomous
//! entities that can communicate with each other using the MEAP protocol.
//! 
//! # Examples
//! 
//! ```rust,no_run
//! use meap_core::{
//!     agent::{Agent, AgentCapability},
//!     connection::ConnectionConfig,
//!     protocol::Protocol,
//! };
//! use std::time::Duration;
//! 
//! # async fn example(protocol: std::sync::Arc<dyn Protocol>) {
//! let config = ConnectionConfig {
//!     max_reconnects: 3,
//!     reconnect_delay: Duration::from_secs(1),
//!     buffer_size: 32,
//! };
//! 
//! let agent = Agent::new(
//!     "agent1".to_string(),
//!     vec![AgentCapability::Chat],
//!     protocol,
//!     config,
//! );
//! 
//! agent.connect("ws://localhost:8080").await.unwrap();
//! # }
//! ```

use crate::error::{Error, Result};
use crate::protocol::{Message, MessageType, Protocol};
use crate::connection::{Connection, ConnectionConfig, ConnectionPool};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};

/// Represents the capabilities that an agent can provide.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentCapability {
    /// Ability to engage in chat-based interactions
    Chat,
    /// Ability to perform search operations
    Search,
    /// Access to vector storage operations
    VectorStore,
    /// Access to graph database operations
    GraphDB,
    /// Memory management capabilities
    Memory,
    /// Deepseek code generation and analysis
    DeepseekCode,
    /// Deepseek language model capabilities  
    DeepseekLLM,
    /// Custom capability with string identifier
    Custom(String),
}

/// Represents the current status of an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentStatus {
    /// Agent is online and ready to process messages
    Online,
    /// Agent is offline or unreachable
    Offline,
    /// Agent is online but currently processing tasks
    Busy,
    /// Agent is in an error state with error message
    Error(String),
}

/// Core agent structure representing a MEAP agent.
pub struct Agent {
    /// Unique identifier for the agent
    id: String,
    /// List of agent capabilities
    capabilities: Vec<AgentCapability>,
    /// Current agent status
    status: Arc<RwLock<AgentStatus>>,
    /// Agent metadata as key-value pairs
    metadata: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    /// Connection pool for managing WebSocket connections
    connection_pool: Arc<ConnectionPool>,
    /// Channel for internal message passing
    message_tx: mpsc::Sender<Message>,
    /// Protocol implementation for message handling
    protocol: Arc<dyn Protocol>,
}

impl Agent {
    /// Creates a new agent with the specified configuration.
    /// 
    /// # Arguments
    /// * `id` - Unique identifier for the agent
    /// * `capabilities` - List of agent capabilities
    /// * `protocol` - Protocol implementation for message handling
    /// * `config` - Connection configuration
    pub fn new(
        id: String,
        capabilities: Vec<AgentCapability>,
        protocol: Arc<dyn Protocol>,
        config: ConnectionConfig,
    ) -> Self {
        let (tx, _) = mpsc::channel(32);
        Self {
            id,
            capabilities,
            status: Arc::new(RwLock::new(AgentStatus::Offline)),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            connection_pool: Arc::new(ConnectionPool::new(config)),
            message_tx: tx,
            protocol,
        }
    }

    /// Connects the agent to a MEAP server.
    /// 
    /// # Arguments
    /// * `url` - WebSocket URL of the MEAP server
    pub async fn connect(&self, url: String) -> Result<()> {
        self.connection_pool.add_connection(self.id.clone(), url).await?;
        let mut status = self.status.write().await;
        *status = AgentStatus::Online;
        Ok(())
    }

    /// Sends a message to another agent.
    /// 
    /// # Arguments
    /// * `to` - Recipient agent ID
    /// * `content` - Message content as JSON
    pub async fn send_message(&self, to: String, content: serde_json::Value) -> Result<()> {
        let message = Message::new(
            MessageType::Request,
            self.id.clone(),
            to,
            content,
        );

        self.protocol.validate_message(&message).await?;

        if let Some(processed) = self.protocol.process_message(message).await? {
            let connections = self.connection_pool.connections.read().await;
            if let Some(conn) = connections.get(&self.id) {
                conn.send(processed).await?;
            } else {
                return Err(Error::Connection("Not connected".into()));
            }
        }

        Ok(())
    }

    /// Updates the agent's status.
    pub async fn set_status(&self, status: AgentStatus) {
        let mut current = self.status.write().await;
        *current = status;
    }

    /// Adds metadata to the agent.
    pub async fn add_metadata(&self, key: String, value: serde_json::Value) {
        let mut metadata = self.metadata.write().await;
        metadata.insert(key, value);
    }

    /// Returns the agent's ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the agent's capabilities.
    pub fn capabilities(&self) -> &[AgentCapability] {
        &self.capabilities
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_agent_lifecycle() {
        let config = ConnectionConfig {
            max_reconnects: 3,
            reconnect_delay: Duration::from_secs(1),
            buffer_size: 32,
        };

        // Create mock protocol
        struct MockProtocol;
        #[async_trait]
        impl Protocol for MockProtocol {
            async fn validate_message(&self, _: &Message) -> Result<()> { Ok(()) }
            async fn process_message(&self, m: Message) -> Result<Option<Message>> { Ok(Some(m)) }
            async fn send_message(&self, _: Message) -> Result<()> { Ok(()) }
            async fn handle_stream(&self, _: Message) -> Result<()> { Ok(()) }
        }

        let agent = Agent::new(
            "test-agent".into(),
            vec![AgentCapability::Chat],
            Arc::new(MockProtocol),
            config,
        );

        assert!(matches!(
            *agent.status.read().await,
            AgentStatus::Offline
        ));

        agent.set_status(AgentStatus::Online).await;
        assert!(matches!(
            *agent.status.read().await,
            AgentStatus::Online
        ));

        agent.add_metadata("version".into(), serde_json::json!("1.0")).await;
        let metadata = agent.metadata.read().await;
        assert_eq!(
            metadata.get("version").unwrap(),
            &serde_json::json!("1.0")
        );
    }
}
