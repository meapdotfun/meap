//! MEAP Command Line Interface
//! Provides tools for managing agents and connections

use clap::{Parser, Subcommand};
use console::style;
use dialoguer::{Input, Select};
use meap_core::{
    agent::{Agent, AgentCapability, AgentStatus},
    connection::ConnectionConfig,
    protocol::Protocol,
    security::{SecurityConfig, AuthMethod, TlsConfig},
};
use commands::{CliState, self};
use std::time::Duration;
use tracing::{info, error};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new agent
    Create {
        /// Agent ID
        #[arg(short, long)]
        id: String,
        
        /// Agent capabilities (comma-separated)
        #[arg(short, long)]
        capabilities: String,

        /// Use TLS for connections
        #[arg(long)]
        tls: bool,

        /// Path to TLS certificate
        #[arg(long)]
        cert: Option<String>,

        /// Path to TLS key
        #[arg(long)]
        key: Option<String>,
    },
    
    /// Connect to a MEAP server
    Connect {
        /// Server URL
        #[arg(short, long)]
        url: String,
        
        /// Authentication token
        #[arg(short, long)]
        token: Option<String>,
    },
    
    /// Send a message to an agent
    Send {
        /// Target agent ID
        #[arg(short, long)]
        to: String,
        
        /// Message content (JSON)
        #[arg(short, long)]
        content: String,
    },
    
    /// List active agents
    List,
    
    /// Show agent status
    Status {
        /// Agent ID
        #[arg(short, long)]
        id: String,
    },
}

fn parse_capabilities(caps_str: &str) -> Vec<AgentCapability> {
    caps_str
        .split(',')
        .map(|c| match c.trim() {
            "chat" => AgentCapability::Chat,
            "search" => AgentCapability::Search,
            "vector" => AgentCapability::VectorStore,
            "graph" => AgentCapability::GraphDB,
            "memory" => AgentCapability::Memory,
            custom => AgentCapability::Custom(custom.to_string()),
        })
        .collect()
}

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let config = ConnectionConfig {
        max_reconnects: 3,
        reconnect_delay: Duration::from_secs(1),
        buffer_size: 32,
    };

    let state = CliState::new(config, None)
        .await
        .expect("Failed to initialize CLI state");

    match cli.command {
        Commands::Create { id, capabilities, tls, cert, key } => {
            info!("Creating agent {} with capabilities: {}", id, capabilities);
            
            let caps = parse_capabilities(&capabilities);
            
            let security_config = if tls {
                if let (Some(cert_path), Some(key_path)) = (cert, key) {
                    Some(SecurityConfig {
                        auth_method: AuthMethod::Token("default".to_string()),
                        encrypt_messages: true,
                        tls_config: Some(TlsConfig {
                            cert_path,
                            key_path,
                            ca_certs: None,
                        }),
                        key_rotation_interval: Duration::from_secs(24 * 60 * 60),
                    })
                } else {
                    error!("TLS requires both certificate and key paths");
                    return;
                }
            } else {
                None
            };

            if let Err(e) = commands::create_agent(&state, id, caps, None).await {
                error!("Failed to create agent: {}", e);
            }
        }

        Commands::Connect { url, token } => {
            if let Err(e) = commands::connect_agent(&state, "default".to_string(), url, token).await {
                error!("Failed to connect: {}", e);
            }
        }

        Commands::Send { to, content } => {
            match serde_json::from_str(&content) {
                Ok(json) => {
                    if let Err(e) = commands::send_message(&state, "cli".to_string(), to, json).await {
                        error!("Failed to send message: {}", e);
                    }
                }
                Err(e) => error!("Invalid JSON content: {}", e),
            }
        }

        Commands::List => {
            if let Err(e) = commands::list_agents(&state).await {
                error!("Failed to list agents: {}", e);
            }
        }

        Commands::Status { id } => {
            if let Err(e) = commands::check_status(&state, id).await {
                error!("Failed to check status: {}", e);
            }
        }
    }
}

async fn interactive_protocol_selection() -> Box<dyn Protocol> {
    let protocols = vec!["MongoDB", "Neo4j", "Qdrant", "SQLite"];
    let selection = Select::new()
        .with_prompt("Select storage protocol")
        .items(&protocols)
        .default(0)
        .interact()
        .unwrap();

    match selection {
        0 => {
            let uri: String = Input::new()
                .with_prompt("MongoDB URI")
                .default("mongodb://localhost:27017".into())
                .interact_text()
                .unwrap();
            
            let db: String = Input::new()
                .with_prompt("Database name")
                .default("meap".into())
                .interact_text()
                .unwrap();
            
            // TODO: Return MongoDB protocol
            unimplemented!()
        }
        // TODO: Implement other protocols
        _ => unimplemented!(),
    }
} 