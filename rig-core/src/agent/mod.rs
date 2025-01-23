use crate::error::{Error, Result};
use crate::protocol::{Message, MessageType, Protocol};
use crate::connection::{Connection, ConnectionConfig, ConnectionPool};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentCapability {
    Chat,
    Search,
    VectorStore,
    GraphDB,
    Memory,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentStatus {
    Online,
    Offline,
    Busy,
    Error(String),
}

pub struct Agent {
    id: String,
    capabilities: Vec<AgentCapability>,
    status: Arc<RwLock<AgentStatus>>,
    metadata: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    connection_pool: Arc<ConnectionPool>,
    message_tx: mpsc::Sender<Message>,
    protocol: Arc<dyn Protocol>,
}

impl Agent {
    pub fn new(
        id: String,
        capabilities: Vec<AgentCapability>,
        protocol: Arc<dyn Protocol>,
        config: ConnectionConfig,
    ) -> Self {
        let (tx, _) = mpsc::channel(32); // Message channel for internal communication
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

    pub async fn connect(&self, url: String) -> Result<()> {
        self.connection_pool.add_connection(self.id.clone(), url).await?;
        let mut status = self.status.write().await;
        *status = AgentStatus::Online;
        Ok(())
    }

    pub async fn send_message(&self, to: String, content: serde_json::Value) -> Result<()> {
        let message = Message::new(
            MessageType::Request,
            self.id.clone(),
            to,
            content,
        );

        // Validate message before sending
        self.protocol.validate_message(&message).await?;

        // Process through protocol
        if let Some(processed) = self.protocol.process_message(message).await? {
            // Send through connection pool
            let connections = self.connection_pool.connections.read().await;
            if let Some(conn) = connections.get(&self.id) {
                conn.send(processed).await?;
            } else {
                return Err(Error::Connection("Not connected".into()));
            }
        }

        Ok(())
    }

    pub async fn set_status(&self, status: AgentStatus) {
        let mut current = self.status.write().await;
        *current = status;
    }

    pub async fn add_metadata(&self, key: String, value: serde_json::Value) {
        let mut metadata = self.metadata.write().await;
        metadata.insert(key, value);
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn capabilities(&self) -> &[AgentCapability] {
        &self.capabilities
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    struct MockProtocol;

    #[async_trait]
    impl Protocol for MockProtocol {
        async fn validate_message(&self, _message: &Message) -> Result<()> {
            Ok(())
        }

        async fn process_message(&self, message: Message) -> Result<Option<Message>> {
            Ok(Some(message))
        }

        async fn send_message(&self, _message: Message) -> Result<()> {
            Ok(())
        }

        async fn handle_stream(&self, _message: Message) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_agent_lifecycle() {
        let config = ConnectionConfig {
            max_reconnects: 3,
            reconnect_delay: Duration::from_secs(1),
            buffer_size: 32,
        };

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
