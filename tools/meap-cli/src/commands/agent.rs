//! Agent command implementations for the CLI

use crate::CliState;
use meap_core::{
    agent::{Agent, AgentCapability},
    error::Result,
    security::SecurityConfig,
    protocol::MessageType,
};
use tracing::{info, error};

/// Creates a new MEAP agent with the specified configuration
pub async fn create_agent(
    state: &CliState,
    id: String,
    capabilities: Vec<AgentCapability>,
    security_config: Option<SecurityConfig>,
) -> Result<()> {
    // Check if agent already exists
    let agents = state.active_agents.read().await;
    if agents.contains(&id) {
        error!("Agent {} already exists", id);
        return Ok(());
    }
    drop(agents);

    // Create agent with capabilities
    info!("Creating agent {} with capabilities: {:?}", id, capabilities);

    // Add to active agents list
    let mut agents = state.active_agents.write().await;
    agents.push(id.clone());

    // Initialize security if configured
    if let Some(config) = security_config {
        state.security_manager.add_agent(&id, config).await?;
    }

    info!("Agent {} created successfully", id);
    Ok(())
}

/// Removes an existing agent
pub async fn remove_agent(state: &CliState, id: String) -> Result<()> {
    let mut agents = state.active_agents.write().await;
    if !agents.contains(&id) {
        error!("Agent {} not found", id);
        return Ok(());
    }

    // Remove from active agents
    agents.retain(|a| a != &id);
    
    // Remove from connection pool if connected
    state.connection_pool.remove_connection(&id).await?;

    info!("Agent {} removed successfully", id);
    Ok(())
}

/// Updates agent capabilities
pub async fn update_capabilities(
    state: &CliState,
    id: String,
    capabilities: Vec<AgentCapability>,
) -> Result<()> {
    let agents = state.active_agents.read().await;
    if !agents.contains(&id) {
        error!("Agent {} not found", id);
        return Ok(());
    }

    info!("Updating capabilities for agent {}: {:?}", id, capabilities);
    // TODO: Implement capability update logic

    Ok(())
} 