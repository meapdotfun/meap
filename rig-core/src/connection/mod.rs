use crate::error::{Error, Result};
use crate::protocol::{Message, MessageType};
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::time;
use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::Message as WsMessage,
    WebSocketStream,
};
use tracing::{debug, error, info, warn};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub struct Connection {
    id: String,
    last_heartbeat: Instant,
    tx: mpsc::Sender<WsMessage>,
    status: ConnectionStatus,
    config: ConnectionConfig,
}

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub max_reconnects: u32,
    pub reconnect_delay: Duration,
    pub buffer_size: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting { attempts: u32 },
    Failed,
}

impl Connection {
    pub fn new(id: String, tx: mpsc::Sender<WsMessage>, config: ConnectionConfig) -> Self {
        Self {
            id,
            last_heartbeat: Instant::now(),
            tx,
            status: ConnectionStatus::Connected,
            config,
        }
    }

    pub async fn send(&mut self, message: Message) -> Result<()> {
        let text = serde_json::to_string(&message)
            .map_err(|e| Error::Serialization(e.to_string()))?;
        
        self.tx.send(WsMessage::Text(text)).await
            .map_err(|e| Error::Connection(format!("Failed to send message: {}", e)))
    }

    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Instant::now();
    }

    pub fn is_alive(&self) -> bool {
        self.last_heartbeat.elapsed() < CONNECTION_TIMEOUT
    }
}

pub struct ConnectionPool {
    connections: Arc<RwLock<HashMap<String, Connection>>>,
    config: ConnectionConfig,
}

impl ConnectionPool {
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    pub async fn add_connection(&self, id: String, url: String) -> Result<()> {
        let (ws_stream, _) = connect_async(url).await
            .map_err(|e| Error::Connection(format!("Failed to connect: {}", e)))?;
        
        let (write, read) = ws_stream.split();
        let (tx, rx) = mpsc::channel(self.config.buffer_size);
        
        let connection = Connection::new(id.clone(), tx, self.config.clone());
        
        let mut connections = self.connections.write().await;
        connections.insert(id, connection);

        // Spawn connection handler tasks
        self.spawn_message_handler(read, id.clone());
        self.spawn_writer_handler(write, rx);
        
        Ok(())
    }

    fn spawn_message_handler(
        &self,
        mut read: impl StreamExt<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>> + Send + 'static,
        id: String,
    ) {
        let connections = self.connections.clone();
        
        tokio::spawn(async move {
            while let Some(message) = read.next().await {
                match message {
                    Ok(WsMessage::Text(text)) => {
                        if let Ok(msg) = serde_json::from_str::<Message>(&text) {
                            if msg.message_type == MessageType::Heartbeat {
                                if let Some(conn) = connections.write().await.get_mut(&id) {
                                    conn.update_heartbeat();
                                }
                            }
                        }
                    }
                    Ok(WsMessage::Close(_)) => {
                        warn!("Connection closed for {}", id);
                        break;
                    }
                    Err(e) => {
                        error!("Error reading message: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        });
    }

    fn spawn_writer_handler(
        &self,
        mut write: impl SinkExt<WsMessage> + Send + 'static,
        mut rx: mpsc::Receiver<WsMessage>,
    ) {
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                if let Err(e) = write.send(message).await {
                    error!("Error sending message: {}", e);
                    break;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_connection_lifecycle() {
        let (tx, _rx) = mpsc::channel(32);
        let config = ConnectionConfig {
            max_reconnects: 3,
            reconnect_delay: Duration::from_secs(1),
            buffer_size: 32,
        };
        
        let mut conn = Connection::new("test".to_string(), tx, config);
        assert!(conn.is_alive());
        
        // Simulate time passing
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(conn.is_alive());
        
        conn.update_heartbeat();
        assert!(conn.is_alive());
    }
}
