//! Message sending and handling commands for the CLI

use crate::CliState;
use meap_core::{
    error::Result,
    protocol::{Message, MessageType},
};
use tracing::{info, error};
use uuid::Uuid;
use std::time::{SystemTime, UNIX_EPOCH};

/// Sends a message from one agent to another
pub async fn send_message(
    state: &CliState,
    from: String,
    to: String,
    content: serde_json::Value,
) -> Result<()> {
    // Create message
    let message = Message {
        id: Uuid::new_v4().to_string(),
        message_type: MessageType::Request,
        from: from.clone(),
        to: to.clone(),
        content,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        correlation_id: None,
        metadata: None,
    };

    // Verify sender exists
    let agents = state.active_agents.read().await;
    if !agents.contains(&from) {
        error!("Sender agent {} not found", from);
        return Ok(());
    }

    // Get connection and send message
    let connections = state.connection_pool.connections().read().await;
    if let Some(conn) = connections.get(&to) {
        conn.send(message).await?;
        info!("Message sent successfully from {} to {}", from, to);
    } else {
        error!("No connection found for recipient agent {}", to);
    }

    Ok(())
}

/// Broadcasts a message to all connected agents
pub async fn broadcast_message(
    state: &CliState,
    from: String,
    content: serde_json::Value,
) -> Result<()> {
    let connections = state.connection_pool.connections().read().await;
    if connections.is_empty() {
        info!("No connected agents to broadcast to");
        return Ok(());
    }

    for (to, conn) in connections.iter() {
        if to != &from {
            let message = Message {
                id: Uuid::new_v4().to_string(),
                message_type: MessageType::Request,
                from: from.clone(),
                to: to.clone(),
                content: content.clone(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                correlation_id: None,
                metadata: None,
            };

            if let Err(e) = conn.send(message).await {
                error!("Failed to send broadcast to {}: {}", to, e);
            }
        }
    }

    info!("Broadcast message sent successfully");
    Ok(())
} 