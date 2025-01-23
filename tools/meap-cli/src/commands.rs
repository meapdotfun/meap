//! Command handlers for the MEAP CLI

use meap_core::{
    agent::{Agent, AgentCapability, AgentStatus},
    connection::{ConnectionConfig, ConnectionPool},
    protocol::{Message, MessageType, Protocol},
    security::{SecurityConfig, SecurityManager},
    error::Result,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error, warn};

/// State shared between commands
pub struct CliState {
    pub connection_pool: Arc<ConnectionPool>,
    pub security_manager: Arc<SecurityManager>,
    pub active_agents: Arc<RwLock<Vec<String>>>,
}

impl CliState {
    pub async fn new(conn_config: ConnectionConfig, security_config: Option<SecurityConfig>) -> Result<Self> {
        let connection_pool = Arc::new(ConnectionPool::new(conn_config));
        let security_manager = if let Some(config) = security_config {
            Arc::new(SecurityManager::new(config).await?)
        } else {
            Arc::new(SecurityManager::new(SecurityConfig::default()).await?)
        };

        Ok(Self {
            connection_pool,
            security_manager,
            active_agents: Arc::new(RwLock::new(Vec::new())),
        })
    }
}

pub async fn create_agent(
    state: &CliState,
    id: String,
    capabilities: Vec<AgentCapability>,
    url: Option<String>,
) -> Result<()> {
    let mut agents = state.active_agents.write().await;
    if agents.contains(&id) {
        warn!("Agent {} already exists", id);
        return Ok(());
    }

    // Add to active agents
    agents.push(id.clone());

    // Connect if URL provided
    if let Some(url) = url {
        state.connection_pool.add_connection(id.clone(), url).await?;
    }

    info!("Agent {} created with capabilities: {:?}", id, capabilities);
    Ok(())
}

pub async fn connect_agent(
    state: &CliState,
    id: String,
    url: String,
    token: Option<String>,
) -> Result<()> {
    // Verify agent exists
    let agents = state.active_agents.read().await;
    if !agents.contains(&id) {
        error!("Agent {} not found", id);
        return Ok(());
    }

    // Authenticate if token provided
    if let Some(token) = token {
        // TODO: Implement authentication
    }

    // Connect agent
    state.connection_pool.add_connection(id.clone(), url).await?;
    info!("Agent {} connected successfully", id);
    Ok(())
}

pub async fn send_message(
    state: &CliState,
    from: String,
    to: String,
    content: Value,
) -> Result<()> {
    let message = Message {
        id: uuid::Uuid::new_v4().to_string(),
        message_type: MessageType::Request,
        from,
        to: to.clone(),
        content,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        correlation_id: None,
        metadata: None,
    };

    // Get connection and send message
    let connections = state.connection_pool.connections().read().await;
    if let Some(conn) = connections.get(&to) {
        conn.send(message).await?;
        info!("Message sent successfully");
    } else {
        error!("No connection found for agent {}", to);
    }

    Ok(())
}

pub async fn list_agents(state: &CliState) -> Result<()> {
    let agents = state.active_agents.read().await;
    if agents.is_empty() {
        info!("No active agents");
        return Ok(());
    }

    info!("Active agents:");
    for agent in agents.iter() {
        let connections = state.connection_pool.connections().read().await;
        let status = if connections.contains_key(agent) {
            "Connected"
        } else {
            "Disconnected"
        };
        info!("  {} - {}", agent, status);
    }

    Ok(())
}

pub async fn check_status(state: &CliState, id: String) -> Result<()> {
    let agents = state.active_agents.read().await;
    if !agents.contains(&id) {
        error!("Agent {} not found", id);
        return Ok(());
    }

    let connections = state.connection_pool.connections().read().await;
    if let Some(conn) = connections.get(&id) {
        info!("Agent {} status:", id);
        info!("  Connection: {}", if conn.is_alive() { "Active" } else { "Inactive" });
        // TODO: Add more status information
    } else {
        info!("Agent {} is not connected", id);
    }

    Ok(())
} 