//! Status checking commands for the CLI

use crate::CliState;
use meap_core::error::Result;
use tracing::{info, error};

/// Lists all active agents and their status
pub async fn list_agents(state: &CliState) -> Result<()> {
    let agents = state.active_agents.read().await;
    if agents.is_empty() {
        info!("No active agents");
        return Ok(());
    }

    info!("Active agents:");
    for agent_id in agents.iter() {
        let connections = state.connection_pool.connections().read().await;
        let status = if let Some(conn) = connections.get(agent_id) {
            if conn.is_alive() {
                "Connected"
            } else {
                "Disconnected"
            }
        } else {
            "Not Connected"
        };

        info!("  {} - {}", agent_id, status);
    }

    Ok(())
}

/// Checks detailed status of a specific agent
pub async fn check_status(state: &CliState, id: String) -> Result<()> {
    let agents = state.active_agents.read().await;
    if !agents.contains(&id) {
        error!("Agent {} not found", id);
        return Ok(());
    }

    let connections = state.connection_pool.connections().read().await;
    if let Some(conn) = connections.get(&id) {
        let metrics = conn.metrics().get_metrics();
        
        info!("Status for agent {}:", id);
        info!("  Connection: {}", if conn.is_alive() { "Active" } else { "Inactive" });
        info!("  Messages sent: {}", metrics.messages_sent);
        info!("  Messages received: {}", metrics.messages_received);
        info!("  Errors: {}", metrics.errors);
        info!("  Average latency: {:?}", metrics.latency);
        info!("  Last active: {:?} ago", metrics.last_active.elapsed());
    } else {
        info!("Agent {} is not connected", id);
    }

    Ok(())
}

/// Gets health status of all connections
pub async fn health_check(state: &CliState) -> Result<()> {
    let connections = state.connection_pool.connections().read().await;
    if connections.is_empty() {
        info!("No active connections to check");
        return Ok(());
    }

    let mut healthy = 0;
    let mut unhealthy = 0;

    for (id, conn) in connections.iter() {
        if conn.is_alive() {
            healthy += 1;
        } else {
            unhealthy += 1;
            error!("Unhealthy connection detected for agent {}", id);
        }
    }

    info!(
        "Health check complete - Healthy: {}, Unhealthy: {}",
        healthy, unhealthy
    );

    Ok(())
} 