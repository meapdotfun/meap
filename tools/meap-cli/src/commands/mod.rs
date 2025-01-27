//! Command implementations for the MEAP CLI tool.
//! 
//! This module contains the implementation of various CLI commands for managing
//! MEAP agents, connections, and debugging functionality.

mod agent;
mod connection;
mod message;
mod status;

pub use agent::*;
pub use connection::*;
pub use message::*;
pub use status::*;

use crate::CliState;
use meap_core::error::Result;

/// Handles the create agent command
pub async fn create_agent(
    state: &CliState,
    id: String,
    capabilities: Vec<AgentCapability>,
    security_config: Option<SecurityConfig>,
) -> Result<()> {
    agent::create_agent(state, id, capabilities, security_config).await
}

/// Handles the connect command
pub async fn connect_agent(
    state: &CliState,
    id: String,
    url: String,
    token: Option<String>,
) -> Result<()> {
    connection::connect_agent(state, id, url, token).await
}

/// Handles the send message command
pub async fn send_message(
    state: &CliState,
    from: String,
    to: String,
    content: serde_json::Value,
) -> Result<()> {
    message::send_message(state, from, to, content).await
}

/// Handles the list agents command
pub async fn list_agents(state: &CliState) -> Result<()> {
    status::list_agents(state).await
}

/// Handles the check status command
pub async fn check_status(state: &CliState, id: String) -> Result<()> {
    status::check_status(state, id).await
} 