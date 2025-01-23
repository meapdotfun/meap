//! Connection module handles WebSocket connections and connection pooling.
//! 
//! This module provides functionality for:
//! - Managing WebSocket connections
//! - Connection pooling and lifecycle management
//! - Heartbeat monitoring
//! - Automatic reconnection
//! 
//! # Examples
//! 
//! ```rust,no_run
//! use meap_core::connection::{ConnectionConfig, ConnectionPool};
//! use std::time::Duration;
//! 
//! # async fn example() {
//! let config = ConnectionConfig {
//!     max_reconnects: 3,
//!     reconnect_delay: Duration::from_secs(1),
//!     buffer_size: 32,
//! };
//! 
//! let pool = ConnectionPool::new(config);
//! pool.add_connection("agent1".to_string(), "ws://localhost:8080".to_string())
//!     .await
//!     .unwrap();
//! # }
//! ```

use crate::error::{Error, Result};
use crate::protocol::{Message, MessageType};
use crate::security::TlsConfig;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::time;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::Message as WsMessage,
    WebSocketStream,
};
use tracing::{debug, error, info, warn};

mod tls;
mod rate_limit;

pub use rate_limit::{RateLimiter, RateLimitConfig};

/// Duration between heartbeat messages
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
/// Maximum time to wait for a heartbeat response
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);

/// Configuration for connection behavior.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Maximum number of reconnection attempts
    pub max_reconnects: u32,
    /// Delay between reconnection attempts
    pub reconnect_delay: Duration,
    /// Size of message buffers
    pub buffer_size: usize,
    /// Rate limiting configuration
    pub rate_limit: Option<RateLimitConfig>,
}

/// Status of a connection.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    /// Connection is active and healthy
    Connected,
    /// Connection has been lost
    Disconnected,
    /// Attempting to reconnect
    Reconnecting { attempts: u32 },
    /// Connection has permanently failed
    Failed,
}

/// Represents a single WebSocket connection.
#[derive(Debug)]
pub struct Connection {
    /// Unique identifier for the connection
    id: String,
    /// Time of last received heartbeat
    last_heartbeat: Instant,
    /// Channel for sending messages
    tx: mpsc::Sender<WsMessage>,
    /// Current connection status
    status: ConnectionStatus,
    /// Connection configuration
    config: ConnectionConfig,
}

impl Connection {
    /// Creates a new connection with the given configuration.
    pub fn new(id: String, tx: mpsc::Sender<WsMessage>, config: ConnectionConfig) -> Self {
        Self {
            id,
            last_heartbeat: Instant::now(),
            tx,
            status: ConnectionStatus::Connected,
            config,
        }
    }

    /// Sends a message through the connection.
    pub async fn send(&mut self, message: Message) -> Result<()> {
        let text = serde_json::to_string(&message)
            .map_err(|e| Error::Serialization(e.to_string()))?;
        
        self.tx.send(WsMessage::Text(text)).await
            .map_err(|e| Error::Connection(format!("Failed to send message: {}", e)))
    }

    /// Updates the heartbeat timestamp.
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Instant::now();
    }

    /// Checks if the connection is still alive based on heartbeat.
    pub fn is_alive(&self) -> bool {
        self.last_heartbeat.elapsed() < CONNECTION_TIMEOUT
    }
}

/// Manages a pool of WebSocket connections.
pub struct ConnectionPool {
    /// Active connections
    connections: Arc<RwLock<HashMap<String, Connection>>>,
    /// Connection configuration
    config: ConnectionConfig,
    /// Rate limiter for managing request rates
    rate_limiter: Option<RateLimiter>,
}

impl ConnectionPool {
    /// Creates a new connection pool with the given configuration.
    pub fn new(config: ConnectionConfig) -> Self {
        let rate_limiter = config.rate_limit.clone()
            .map(|config| RateLimiter::new(config));

        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            config,
            rate_limiter,
        }
    }

    /// Adds a new connection to the pool.
    pub async fn add_connection(&self, id: String, url: String) -> Result<()> {
        // Check rate limit before establishing connection
        if let Some(limiter) = &self.rate_limiter {
            limiter.check_request(&id).await?;
        }

        let (ws_stream, _) = connect_async(url).await
            .map_err(|e| Error::Connection(format!("Failed to connect: {}", e)))?;
        
        let (write, read) = ws_stream.split();
        let (tx, rx) = mpsc::channel(self.config.buffer_size);
        
        let connection = Connection::new(id.clone(), tx, self.config.clone());
        
        let mut connections = self.connections.write().await;
        connections.insert(id.clone(), connection);

        // Spawn connection handler tasks
        self.spawn_message_handler(read, id.clone());
        self.spawn_writer_handler(write, rx);
        
        Ok(())
    }

    /// Spawns a task to handle incoming messages.
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

    /// Spawns a task to handle outgoing messages.
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

    /// Creates a new secure connection with TLS
    pub async fn add_secure_connection(
        &self,
        id: String,
        url: String,
        tls_config: &TlsConfig
    ) -> Result<()> {
        let acceptor = tls::create_tls_acceptor(tls_config).await?;
        let (ws_stream, _) = connect_async(url).await
            .map_err(|e| Error::Connection(format!("Failed to connect: {}", e)))?;
        
        let tls_stream = acceptor.accept(ws_stream).await
            .map_err(|e| Error::Security(format!("TLS handshake failed: {}", e)))?;
        
        let (write, read) = tls_stream.split();
        let (tx, rx) = mpsc::channel(self.config.buffer_size);
        
        let connection = Connection::new(id.clone(), tx, self.config.clone());
        
        let mut connections = self.connections.write().await;
        connections.insert(id.clone(), connection);

        self.spawn_message_handler(read, id.clone());
        self.spawn_writer_handler(write, rx);
        
        Ok(())
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
            rate_limit: None,
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
