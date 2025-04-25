//! Mobile agent-to-agent communication
//! Handles agent discovery, connection, and message exchange between mobile agents

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

use crate::{
    analytics::AnalyticsManager,
    location::LocationManager,
    media::MediaMetadata,
    network::NetworkManager,
    security::SecurityManager,
    storage::StorageManager,
};

/// Agent capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilities {
    /// Whether agent supports location services
    pub location_enabled: bool,
    /// Whether agent supports camera/media capture
    pub media_enabled: bool,
    /// Whether agent supports biometric authentication
    pub biometric_enabled: bool,
    /// Whether agent supports push notifications
    pub notifications_enabled: bool,
    /// Whether agent supports background processing
    pub background_enabled: bool,
    /// Whether agent supports offline mode
    pub offline_enabled: bool,
    /// Whether agent supports encryption
    pub encryption_enabled: bool,
    /// Whether agent supports file sharing
    pub file_sharing_enabled: bool,
    /// Whether agent supports real-time communication
    pub realtime_enabled: bool,
    /// Custom capabilities
    pub custom: HashMap<String, bool>,
}

/// Agent status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentStatus {
    /// Agent is online and available
    Online,
    /// Agent is online but busy
    Busy,
    /// Agent is online but away
    Away,
    /// Agent is offline
    Offline,
    /// Agent is in error state
    Error,
}

/// Agent connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConnectionState {
    /// Connection is disconnected
    Disconnected,
    /// Connection is connecting
    Connecting,
    /// Connection is connected
    Connected,
    /// Connection is reconnecting
    Reconnecting,
    /// Connection is disconnecting
    Disconnecting,
    /// Connection is in error state
    Error,
}

/// Agent message type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageType {
    /// Text message
    Text,
    /// Media message (photo, video, audio)
    Media,
    /// Location message
    Location,
    /// Command message
    Command,
    /// Status update message
    Status,
    /// Error message
    Error,
    /// Custom message type
    Custom(String),
}

/// Agent message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Message ID
    pub id: String,
    /// Message type
    pub message_type: MessageType,
    /// Sender agent ID
    pub from: String,
    /// Recipient agent ID
    pub to: String,
    /// Message content
    pub content: serde_json::Value,
    /// Message timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Message expiration time
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether message requires acknowledgment
    pub requires_ack: bool,
    /// Whether message is encrypted
    pub encrypted: bool,
    /// Message priority (1-5, 5 being highest)
    pub priority: u8,
    /// Message metadata
    pub metadata: HashMap<String, String>,
}

/// Agent connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConnection {
    /// Connection ID
    pub id: String,
    /// Remote agent ID
    pub remote_agent_id: String,
    /// Connection state
    pub state: ConnectionState,
    /// Connection established time
    pub established_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Last activity time
    pub last_activity: Option<chrono::DateTime<chrono::Utc>>,
    /// Connection capabilities
    pub capabilities: AgentCapabilities,
    /// Connection metadata
    pub metadata: HashMap<String, String>,
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent ID
    pub agent_id: String,
    /// Agent name
    pub agent_name: String,
    /// Agent version
    pub agent_version: String,
    /// Agent capabilities
    pub capabilities: AgentCapabilities,
    /// Whether to enable auto-discovery
    pub auto_discovery: bool,
    /// Whether to enable auto-reconnect
    pub auto_reconnect: bool,
    /// Maximum reconnection attempts
    pub max_reconnect_attempts: u32,
    /// Reconnection delay in seconds
    pub reconnect_delay: Duration,
    /// Message queue size
    pub message_queue_size: usize,
    /// Message timeout in seconds
    pub message_timeout: Duration,
    /// Whether to enable message persistence
    pub message_persistence: bool,
    /// Whether to enable message encryption
    pub message_encryption: bool,
    /// Whether to enable message compression
    pub message_compression: bool,
    /// Whether to enable message validation
    pub message_validation: bool,
    /// Whether to enable message analytics
    pub message_analytics: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            agent_id: format!("mobile-agent-{}", uuid::Uuid::new_v4()),
            agent_name: "Mobile Agent".to_string(),
            agent_version: "1.0.0".to_string(),
            capabilities: AgentCapabilities {
                location_enabled: true,
                media_enabled: true,
                biometric_enabled: true,
                notifications_enabled: true,
                background_enabled: true,
                offline_enabled: true,
                encryption_enabled: true,
                file_sharing_enabled: true,
                realtime_enabled: true,
                custom: HashMap::new(),
            },
            auto_discovery: true,
            auto_reconnect: true,
            max_reconnect_attempts: 5,
            reconnect_delay: Duration::from_secs(5),
            message_queue_size: 100,
            message_timeout: Duration::from_secs(30),
            message_persistence: true,
            message_encryption: true,
            message_compression: true,
            message_validation: true,
            message_analytics: true,
        }
    }
}

/// Mobile agent
pub struct MobileAgent {
    /// Agent configuration
    config: AgentConfig,
    /// Agent status
    status: Arc<RwLock<AgentStatus>>,
    /// Active connections
    connections: Arc<RwLock<HashMap<String, AgentConnection>>>,
    /// Message queue
    message_queue: Arc<RwLock<Vec<AgentMessage>>>,
    /// Message channel
    message_tx: mpsc::Sender<AgentMessage>,
    /// Message receiver
    message_rx: mpsc::Receiver<AgentMessage>,
    /// Analytics manager
    analytics: Option<Arc<AnalyticsManager>>,
    /// Location manager
    location: Option<Arc<LocationManager>>,
    /// Network manager
    network: Option<Arc<NetworkManager>>,
    /// Security manager
    security: Option<Arc<SecurityManager>>,
    /// Storage manager
    storage: Option<Arc<StorageManager>>,
}

impl MobileAgent {
    /// Create a new mobile agent
    pub fn new(config: AgentConfig) -> Self {
        let (message_tx, message_rx) = mpsc::channel(config.message_queue_size);
        
        Self {
            config,
            status: Arc::new(RwLock::new(AgentStatus::Offline)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            message_queue: Arc::new(RwLock::new(Vec::new())),
            message_tx,
            message_rx,
            analytics: None,
            location: None,
            network: None,
            security: None,
            storage: None,
        }
    }
    
    /// Set analytics manager
    pub fn set_analytics(&mut self, analytics: Arc<AnalyticsManager>) {
        self.analytics = Some(analytics);
    }
    
    /// Set location manager
    pub fn set_location(&mut self, location: Arc<LocationManager>) {
        self.location = Some(location);
    }
    
    /// Set network manager
    pub fn set_network(&mut self, network: Arc<NetworkManager>) {
        self.network = Some(network);
    }
    
    /// Set security manager
    pub fn set_security(&mut self, security: Arc<SecurityManager>) {
        self.security = Some(security);
    }
    
    /// Set storage manager
    pub fn set_storage(&mut self, storage: Arc<StorageManager>) {
        self.storage = Some(storage);
    }
    
    /// Initialize the agent
    pub async fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing mobile agent: {}", self.config.agent_id);
        
        // Initialize managers if available
        if let Some(analytics) = &self.analytics {
            analytics.initialize().await?;
        }
        
        if let Some(location) = &self.location {
            location.initialize().await?;
        }
        
        if let Some(network) = &self.network {
            network.initialize().await?;
        }
        
        if let Some(security) = &self.security {
            security.initialize().await?;
        }
        
        if let Some(storage) = &self.storage {
            storage.initialize().await?;
        }
        
        // Set agent status to online
        let mut status = self.status.write().await;
        *status = AgentStatus::Online;
        
        info!("Mobile agent initialized: {}", self.config.agent_id);
        Ok(())
    }
    
    /// Connect to a remote agent
    pub async fn connect(&self, remote_agent_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("Connecting to remote agent: {}", remote_agent_id);
        
        // Check if already connected
        let connections = self.connections.read().await;
        if connections.contains_key(remote_agent_id) {
            return Ok(());
        }
        
        // Create connection
        let connection = AgentConnection {
            id: format!("conn-{}", uuid::Uuid::new_v4()),
            remote_agent_id: remote_agent_id.to_string(),
            state: ConnectionState::Connecting,
            established_at: None,
            last_activity: None,
            capabilities: AgentCapabilities {
                location_enabled: false,
                media_enabled: false,
                biometric_enabled: false,
                notifications_enabled: false,
                background_enabled: false,
                offline_enabled: false,
                encryption_enabled: false,
                file_sharing_enabled: false,
                realtime_enabled: false,
                custom: HashMap::new(),
            },
            metadata: HashMap::new(),
        };
        
        // Add connection
        let mut connections = self.connections.write().await;
        connections.insert(remote_agent_id.to_string(), connection.clone());
        
        // Update connection state
        let mut conn = connections.get_mut(remote_agent_id).unwrap();
        conn.state = ConnectionState::Connected;
        conn.established_at = Some(chrono::Utc::now());
        conn.last_activity = Some(chrono::Utc::now());
        
        info!("Connected to remote agent: {}", remote_agent_id);
        Ok(())
    }
    
    /// Disconnect from a remote agent
    pub async fn disconnect(&self, remote_agent_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("Disconnecting from remote agent: {}", remote_agent_id);
        
        // Check if connected
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.get_mut(remote_agent_id) {
            connection.state = ConnectionState::Disconnecting;
        } else {
            return Ok(());
        }
        
        // Remove connection
        connections.remove(remote_agent_id);
        
        info!("Disconnected from remote agent: {}", remote_agent_id);
        Ok(())
    }
    
    /// Send a message to a remote agent
    pub async fn send_message(&self, to: &str, message_type: MessageType, content: serde_json::Value) -> Result<String, Box<dyn std::error::Error>> {
        info!("Sending message to agent: {}", to);
        
        // Create message
        let message = AgentMessage {
            id: format!("msg-{}", uuid::Uuid::new_v4()),
            message_type,
            from: self.config.agent_id.clone(),
            to: to.to_string(),
            content,
            timestamp: chrono::Utc::now(),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::seconds(self.config.message_timeout.as_secs() as i64)),
            requires_ack: true,
            encrypted: self.config.message_encryption,
            priority: 3,
            metadata: HashMap::new(),
        };
        
        // Encrypt message if needed
        let message = if self.config.message_encryption {
            if let Some(security) = &self.security {
                // Encrypt message content
                let encrypted_content = security.encrypt_data(&serde_json::to_vec(&message.content)?)?;
                let mut encrypted_message = message.clone();
                encrypted_message.content = serde_json::Value::String(base64::encode(encrypted_content));
                encrypted_message.encrypted = true;
                encrypted_message
            } else {
                message
            }
        } else {
            message
        };
        
        // Compress message if needed
        let message = if self.config.message_compression {
            // Compress message content
            let compressed_content = zstd::encode_all(&serde_json::to_vec(&message.content)?, 0)?;
            let mut compressed_message = message.clone();
            compressed_message.content = serde_json::Value::String(base64::encode(compressed_content));
            compressed_message
        } else {
            message
        };
        
        // Add to message queue
        let mut queue = self.message_queue.write().await;
        queue.push(message.clone());
        
        // Send message
        let _ = self.message_tx.send(message.clone()).await;
        
        // Record analytics if enabled
        if self.config.message_analytics {
            if let Some(analytics) = &self.analytics {
                analytics.record_event("message_sent", &HashMap::from([
                    ("message_id".to_string(), message.id.clone()),
                    ("message_type".to_string(), format!("{:?}", message.message_type)),
                    ("from".to_string(), message.from.clone()),
                    ("to".to_string(), message.to.clone()),
                ])).await?;
            }
        }
        
        info!("Message sent to agent: {}", to);
        Ok(message.id)
    }
    
    /// Process incoming messages
    pub async fn process_messages(&mut self) {
        while let Some(message) = self.message_rx.recv().await {
            info!("Processing message: {:?}", message);
            
            // Decrypt message if needed
            let message = if message.encrypted {
                if let Some(security) = &self.security {
                    // Decrypt message content
                    let encrypted_content = base64::decode(message.content.as_str().unwrap())?;
                    let decrypted_content = security.decrypt_data(&encrypted_content)?;
                    let mut decrypted_message = message.clone();
                    decrypted_message.content = serde_json::from_slice(&decrypted_content)?;
                    decrypted_message.encrypted = false;
                    decrypted_message
                } else {
                    message
                }
            } else {
                message
            };
            
            // Decompress message if needed
            let message = if message.content.is_string() && message.content.as_str().unwrap().starts_with("eJ") {
                // Decompress message content
                let compressed_content = base64::decode(message.content.as_str().unwrap())?;
                let decompressed_content = zstd::decode_all(&compressed_content)?;
                let mut decompressed_message = message.clone();
                decompressed_message.content = serde_json::from_slice(&decompressed_content)?;
                decompressed_message
            } else {
                message
            };
            
            // Handle message based on type
            match message.message_type {
                MessageType::Text => {
                    info!("Received text message: {}", message.content);
                }
                MessageType::Media => {
                    info!("Received media message: {:?}", message.content);
                }
                MessageType::Location => {
                    info!("Received location message: {:?}", message.content);
                }
                MessageType::Command => {
                    info!("Received command message: {:?}", message.content);
                }
                MessageType::Status => {
                    info!("Received status message: {:?}", message.content);
                }
                MessageType::Error => {
                    warn!("Received error message: {:?}", message.content);
                }
                MessageType::Custom(ref custom_type) => {
                    info!("Received custom message of type {}: {:?}", custom_type, message.content);
                }
            }
            
            // Record analytics if enabled
            if self.config.message_analytics {
                if let Some(analytics) = &self.analytics {
                    let _ = analytics.record_event("message_received", &HashMap::from([
                        ("message_id".to_string(), message.id.clone()),
                        ("message_type".to_string(), format!("{:?}", message.message_type)),
                        ("from".to_string(), message.from.clone()),
                        ("to".to_string(), message.to.clone()),
                    ])).await;
                }
            }
        }
    }
    
    /// Discover nearby agents
    pub async fn discover_agents(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        info!("Discovering nearby agents");
        
        // Check if auto-discovery is enabled
        if !self.config.auto_discovery {
            return Ok(Vec::new());
        }
        
        // Use network manager to discover agents
        if let Some(network) = &self.network {
            let discovered_agents = network.discover_peers().await?;
            info!("Discovered {} agents", discovered_agents.len());
            return Ok(discovered_agents);
        }
        
        Ok(Vec::new())
    }
    
    /// Share location with a remote agent
    pub async fn share_location(&self, to: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("Sharing location with agent: {}", to);
        
        // Check if location is enabled
        if !self.config.capabilities.location_enabled {
            return Err("Location is not enabled".into());
        }
        
        // Get current location
        if let Some(location) = &self.location {
            let location_data = location.get_current_location().await?;
            
            // Send location message
            self.send_message(
                to,
                MessageType::Location,
                serde_json::json!({
                    "latitude": location_data.latitude,
                    "longitude": location_data.longitude,
                    "altitude": location_data.altitude,
                    "accuracy": location_data.accuracy,
                    "speed": location_data.speed,
                    "bearing": location_data.bearing,
                    "timestamp": location_data.timestamp,
                }),
            ).await?;
            
            info!("Location shared with agent: {}", to);
            return Ok(());
        }
        
        Err("Location manager not available".into())
    }
    
    /// Share media with a remote agent
    pub async fn share_media(&self, to: &str, media: MediaMetadata) -> Result<(), Box<dyn std::error::Error>> {
        info!("Sharing media with agent: {}", to);
        
        // Check if media is enabled
        if !self.config.capabilities.media_enabled {
            return Err("Media is not enabled".into());
        }
        
        // Send media message
        self.send_message(
            to,
            MessageType::Media,
            serde_json::json!({
                "media_type": format!("{:?}", media.media_type),
                "file_path": media.file_path.to_string_lossy(),
                "file_size": media.file_size,
                "created_at": media.created_at,
                "duration": media.duration,
                "width": media.width,
                "height": media.height,
                "bitrate": media.bitrate,
                "frame_rate": media.frame_rate,
            }),
        ).await?;
        
        info!("Media shared with agent: {}", to);
        Ok(())
    }
    
    /// Send a command to a remote agent
    pub async fn send_command(&self, to: &str, command: &str, params: HashMap<String, String>) -> Result<String, Box<dyn std::error::Error>> {
        info!("Sending command to agent: {}", to);
        
        // Send command message
        let message_id = self.send_message(
            to,
            MessageType::Command,
            serde_json::json!({
                "command": command,
                "params": params,
            }),
        ).await?;
        
        info!("Command sent to agent: {}", to);
        Ok(message_id)
    }
    
    /// Update agent status
    pub async fn update_status(&self, status: AgentStatus) -> Result<(), Box<dyn std::error::Error>> {
        info!("Updating agent status: {:?}", status);
        
        // Update status
        let mut current_status = self.status.write().await;
        *current_status = status;
        
        // Broadcast status update to all connections
        let connections = self.connections.read().await;
        for (remote_agent_id, _) in connections.iter() {
            self.send_message(
                remote_agent_id,
                MessageType::Status,
                serde_json::json!({
                    "agent_id": self.config.agent_id,
                    "status": format!("{:?}", status),
                    "timestamp": chrono::Utc::now(),
                }),
            ).await?;
        }
        
        info!("Agent status updated: {:?}", status);
        Ok(())
    }
    
    /// Get agent status
    pub async fn get_status(&self) -> AgentStatus {
        let status = self.status.read().await;
        *status
    }
    
    /// Get agent connections
    pub async fn get_connections(&self) -> Vec<AgentConnection> {
        let connections = self.connections.read().await;
        connections.values().cloned().collect()
    }
    
    /// Get agent messages
    pub async fn get_messages(&self) -> Vec<AgentMessage> {
        let queue = self.message_queue.read().await;
        queue.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_agent_initialization() {
        let config = AgentConfig::default();
        let agent = MobileAgent::new(config.clone());
        
        assert_eq!(agent.config.agent_id, config.agent_id);
        assert_eq!(agent.config.agent_name, config.agent_name);
        assert_eq!(agent.config.agent_version, config.agent_version);
    }
    
    #[tokio::test]
    async fn test_agent_connection() {
        let config = AgentConfig::default();
        let agent = MobileAgent::new(config);
        
        // Connect to remote agent
        agent.connect("remote-agent-1").await.unwrap();
        
        // Check connection
        let connections = agent.get_connections().await;
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].remote_agent_id, "remote-agent-1");
        assert_eq!(connections[0].state, ConnectionState::Connected);
        
        // Disconnect from remote agent
        agent.disconnect("remote-agent-1").await.unwrap();
        
        // Check connection
        let connections = agent.get_connections().await;
        assert_eq!(connections.len(), 0);
    }
    
    #[tokio::test]
    async fn test_agent_message() {
        let config = AgentConfig::default();
        let mut agent = MobileAgent::new(config);
        
        // Send message
        let message_id = agent.send_message(
            "remote-agent-1",
            MessageType::Text,
            serde_json::json!("Hello, world!"),
        ).await.unwrap();
        
        // Check message
        let messages = agent.get_messages().await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, message_id);
        assert_eq!(messages[0].message_type, MessageType::Text);
        assert_eq!(messages[0].from, agent.config.agent_id);
        assert_eq!(messages[0].to, "remote-agent-1");
        assert_eq!(messages[0].content, serde_json::json!("Hello, world!"));
    }
    
    #[tokio::test]
    async fn test_agent_status() {
        let config = AgentConfig::default();
        let agent = MobileAgent::new(config);
        
        // Initialize agent
        agent.initialize().await.unwrap();
        
        // Check initial status
        let status = agent.get_status().await;
        assert_eq!(status, AgentStatus::Online);
        
        // Update status
        agent.update_status(AgentStatus::Busy).await.unwrap();
        
        // Check updated status
        let status = agent.get_status().await;
        assert_eq!(status, AgentStatus::Busy);
    }
} 