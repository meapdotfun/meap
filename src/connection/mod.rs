use crate::error::{MeapError, Result};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time;
use tokio_tungstenite::tungstenite::Message as WsMessage;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Connection {
    id: String,
    last_heartbeat: Instant,
    tx: mpsc::Sender<WsMessage>,
    reconnect_attempts: u32,
    max_reconnects: u32,
}

impl Connection {
    pub fn new(
        id: String, 
        tx: mpsc::Sender<WsMessage>,
        max_reconnects: u32,
    ) -> Self {
        Self {
            id,
            last_heartbeat: Instant::now(),
            tx,
            reconnect_attempts: 0,
            max_reconnects,
        }
    }

    pub async fn send(&mut self, message: WsMessage) -> Result<()> {
        self.tx.send(message).await.map_err(|e| {
            MeapError::Connection(format!("Failed to send message: {}", e))
        })
    }

    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Instant::now();
    }

    pub fn is_alive(&self) -> bool {
        self.last_heartbeat.elapsed() < CONNECTION_TIMEOUT
    }

    pub async fn try_reconnect(&mut self) -> Result<bool> {
        if self.reconnect_attempts >= self.max_reconnects {
            return Ok(false);
        }

        // Exponential backoff
        let delay = Duration::from_secs(2u64.pow(self.reconnect_attempts));
        time::sleep(delay).await;
        
        self.reconnect_attempts += 1;
        Ok(true)
    }

    pub fn reset_reconnect_attempts(&mut self) {
        self.reconnect_attempts = 0;
    }
}

pub struct ConnectionPool {
    connections: tokio::sync::RwLock<std::collections::HashMap<String, Connection>>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            connections: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub async fn add(&self, connection: Connection) {
        let mut connections = self.connections.write().await;
        connections.insert(connection.id.clone(), connection);
    }

    pub async fn remove(&self, id: &str) -> Option<Connection> {
        let mut connections = self.connections.write().await;
        connections.remove(id)
    }

    pub async fn get(&self, id: &str) -> Option<tokio::sync::RwLockWriteGuard<'_, Connection>> {
        let connections = self.connections.write().await;
        connections.get_mut(id)
    }

    pub async fn check_connections(&self) {
        let mut connections = self.connections.write().await;
        connections.retain(|_, conn| conn.is_alive());
    }
} 