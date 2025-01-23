use crate::{Message, MessageType, AgentStatus};
use futures::{StreamExt, SinkExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio::net::{TcpListener, TcpStream};

// Represents a connected agent
struct ConnectedAgent {
    id: String,
    status: AgentStatus,
    capabilities: Vec<String>,
}

// Main server struct
pub struct MeapServer {
    agents: Arc<RwLock<HashMap<String, ConnectedAgent>>>,
    connections: Arc<RwLock<HashMap<String, tokio::sync::mpsc::Sender<WsMessage>>>>,
}

impl MeapServer {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(&self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(addr).await?;
        println!("MEAP Server listening on: {}", addr);

        while let Ok((stream, _)) = listener.accept().await {
            let agents = self.agents.clone();
            let connections = self.connections.clone();
            
            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(stream, agents, connections).await {
                    eprintln!("Error handling connection: {}", e);
                }
            });
        }

        Ok(())
    }

    async fn handle_connection(
        stream: TcpStream,
        agents: Arc<RwLock<HashMap<String, ConnectedAgent>>>,
        connections: Arc<RwLock<HashMap<String, tokio::sync::mpsc::Sender<WsMessage>>>>
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ws_stream = tokio_tungstenite::accept_async(stream).await?;
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let (tx, mut rx) = tokio::sync::mpsc::channel(32);

        // Handle incoming messages
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(WsMessage::Text(text)) => {
                    if let Ok(message) = serde_json::from_str::<Message>(&text) {
                        // Handle the first message as registration
                        if !agents.read().await.contains_key(&message.from) {
                            let agent = ConnectedAgent {
                                id: message.from.clone(),
                                status: AgentStatus::Online,
                                capabilities: vec![],
                            };
                            agents.write().await.insert(message.from.clone(), agent);
                            connections.write().await.insert(message.from.clone(), tx.clone());
                            continue;
                        }

                        // Route message to target agent
                        if let Some(target_tx) = connections.read().await.get(&message.to) {
                            let _ = target_tx.send(WsMessage::Text(text)).await;
                        }
                    }
                }
                Ok(WsMessage::Close(_)) => break,
                _ => {}
            }
        }

        Ok(())
    }
} 