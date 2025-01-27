//! Connection management commands for the CLI

use crate::CliState;
use meap_core::{
    error::Result,
    protocol::MessageType,
};
use tracing::{info, error, warn};
use std::time::Duration;

/// Connects an agent to a MEAP server
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

    // Check if already connected
    let connections = state.connection_pool.connections().read().await;
    if connections.contains_key(&id) {
        warn!("Agent {} is already connected", id);
        return Ok(());
    }
    drop(connections);

    // Authenticate if token provided
    if let Some(token) = token {
        info!("Authenticating agent {}", id);
        state.security_manager.authenticate_agent(&id, &token).await?;
    }

    // Add connection to pool
    info!("Connecting agent {} to {}", id, url);
    state.connection_pool.add_connection(id.clone(), url).await?;

    // Wait for connection to establish
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify connection status
    let connections = state.connection_pool.connections().read().await;
    if let Some(conn) = connections.get(&id) {
        if conn.is_alive() {
            info!("Agent {} connected successfully", id);
        } else {
            error!("Failed to establish connection for agent {}", id);
        }
    }

    Ok(())
}

/// Disconnects an agent from the server
pub async fn disconnect_agent(state: &CliState, id: String) -> Result<()> {
    let connections = state.connection_pool.connections().read().await;
    if !connections.contains_key(&id) {
        warn!("Agent {} is not connected", id);
        return Ok(());
    }
    drop(connections);

    info!("Disconnecting agent {}", id);
    state.connection_pool.remove_connection(&id).await?;
    info!("Agent {} disconnected successfully", id);

    Ok(())
}

/// Lists all active connections
pub async fn list_connections(state: &CliState) -> Result<()> {
    let connections = state.connection_pool.connections().read().await;
    if connections.is_empty() {
        info!("No active connections");
        return Ok(());
    }

    info!("Active connections:");
    for (id, conn) in connections.iter() {
        let status = if conn.is_alive() { "Connected" } else { "Disconnected" };
        info!("  {} - {}", id, status);
    }

    Ok(())
} 