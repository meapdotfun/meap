//! Mobile app network requests and API communication
//! Handles HTTP requests, offline capabilities, and request retry mechanisms

use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

/// HTTP request method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HttpMethod {
    /// GET request
    Get,
    /// POST request
    Post,
    /// PUT request
    Put,
    /// DELETE request
    Delete,
    /// PATCH request
    Patch,
}

/// Request priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RequestPriority {
    /// High priority - immediate processing
    High,
    /// Normal priority - standard processing
    Normal,
    /// Low priority - batched processing
    Low,
}

/// Request configuration
#[derive(Debug, Clone)]
pub struct RequestConfig {
    /// Request method
    pub method: HttpMethod,
    /// Request URL
    pub url: String,
    /// Request headers
    pub headers: HashMap<String, String>,
    /// Request body
    pub body: Option<Vec<u8>>,
    /// Request timeout
    pub timeout: Duration,
    /// Request priority
    pub priority: RequestPriority,
    /// Whether to enable caching
    pub enable_cache: bool,
    /// Cache TTL
    pub cache_ttl: Option<Duration>,
    /// Whether to retry on failure
    pub enable_retry: bool,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Retry delay
    pub retry_delay: Duration,
}

/// Request status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RequestStatus {
    /// Request is pending
    Pending,
    /// Request is in progress
    InProgress,
    /// Request completed successfully
    Completed,
    /// Request failed
    Failed,
    /// Request was cancelled
    Cancelled,
}

/// Network request
#[derive(Debug, Clone)]
pub struct NetworkRequest {
    /// Unique request ID
    pub id: String,
    /// Request configuration
    pub config: RequestConfig,
    /// Request status
    pub status: RequestStatus,
    /// Creation timestamp
    pub created_at: Instant,
    /// Last update timestamp
    pub updated_at: Instant,
    /// Response data
    pub response: Option<Vec<u8>>,
    /// Error message
    pub error: Option<String>,
    /// Retry attempts
    pub retry_attempts: u32,
    /// Last retry timestamp
    pub last_retry: Option<Instant>,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Base URL for API requests
    pub base_url: String,
    /// Default request timeout
    pub default_timeout: Duration,
    /// Maximum concurrent requests
    pub max_concurrent_requests: usize,
    /// Whether to enable offline mode
    pub enable_offline_mode: bool,
    /// Offline queue size
    pub offline_queue_size: usize,
    /// Background sync interval
    pub sync_interval: Duration,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Retry delay
    pub retry_delay: Duration,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.example.com".to_string(),
            default_timeout: Duration::from_secs(30),
            max_concurrent_requests: 10,
            enable_offline_mode: true,
            offline_queue_size: 100,
            sync_interval: Duration::from_secs(300), // 5 minutes
            max_retries: 3,
            retry_delay: Duration::from_secs(1),
        }
    }
}

/// Network manager
pub struct NetworkManager {
    /// Network configuration
    config: NetworkConfig,
    /// Request storage
    requests: Arc<RwLock<HashMap<String, NetworkRequest>>>,
    /// Request queue
    request_queue: Arc<RwLock<VecDeque<String>>>,
    /// Background task channel
    background_tx: mpsc::Sender<BackgroundTask>,
    /// Platform-specific network handlers
    handlers: Arc<RwLock<HashMap<String, Box<dyn NetworkHandler>>>>,
    /// Offline request queue
    offline_queue: Arc<RwLock<VecDeque<String>>>,
}

/// Background task types
#[derive(Debug)]
pub enum BackgroundTask {
    /// Process request queue
    ProcessQueue,
    /// Sync offline queue
    SyncOffline,
    /// Clean up completed requests
    Cleanup,
}

/// Network handler trait
#[async_trait::async_trait]
pub trait NetworkHandler: Send + Sync {
    /// Initialize the network handler
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Check network connectivity
    async fn check_connectivity(&self) -> Result<bool, Box<dyn std::error::Error>>;
    
    /// Execute network request
    async fn execute_request(&self, request: &NetworkRequest) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
    
    /// Cancel network request
    async fn cancel_request(&self, request_id: &str) -> Result<(), Box<dyn std::error::Error>>;
}

impl NetworkManager {
    /// Create a new network manager
    pub fn new(config: NetworkConfig) -> Self {
        let (background_tx, background_rx) = mpsc::channel(100);
        
        let manager = Self {
            config,
            requests: Arc::new(RwLock::new(HashMap::new())),
            request_queue: Arc::new(RwLock::new(VecDeque::new())),
            background_tx,
            handlers: Arc::new(RwLock::new(HashMap::new())),
            offline_queue: Arc::new(RwLock::new(VecDeque::new())),
        };
        
        // Start background task
        tokio::spawn(manager.run_background_task(background_rx));
        
        manager
    }
    
    /// Register a network handler
    pub async fn register_handler<H>(&self, platform: &str, handler: H)
    where
        H: NetworkHandler + 'static,
    {
        let mut handlers = self.handlers.write().await;
        handlers.insert(platform.to_string(), Box::new(handler));
    }
    
    /// Execute a network request
    pub async fn execute<T>(&self, config: RequestConfig) -> Result<T, Box<dyn std::error::Error>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Instant::now();
        
        let request = NetworkRequest {
            id: id.clone(),
            config,
            status: RequestStatus::Pending,
            created_at: now,
            updated_at: now,
            response: None,
            error: None,
            retry_attempts: 0,
            last_retry: None,
        };
        
        // Store request
        {
            let mut requests = self.requests.write().await;
            requests.insert(id.clone(), request);
        }
        
        // Check connectivity
        let handlers = self.handlers.read().await;
        let mut is_online = false;
        for handler in handlers.values() {
            if let Ok(online) = handler.check_connectivity().await {
                is_online = online;
                break;
            }
        }
        
        if !is_online && self.config.enable_offline_mode {
            // Queue for offline processing
            let mut offline_queue = self.offline_queue.write().await;
            offline_queue.push_back(id.clone());
            
            // Trigger background sync
            self.background_tx.send(BackgroundTask::SyncOffline).await?;
            
            return Err("Network is offline. Request queued.".into());
        }
        
        // Add to request queue
        {
            let mut queue = self.request_queue.write().await;
            queue.push_back(id.clone());
        }
        
        // Trigger request processing
        self.background_tx.send(BackgroundTask::ProcessQueue).await?;
        
        // Wait for response
        loop {
            let requests = self.requests.read().await;
            if let Some(request) = requests.get(&id) {
                match request.status {
                    RequestStatus::Completed => {
                        if let Some(response) = &request.response {
                            return Ok(serde_json::from_slice(response)?);
                        }
                    }
                    RequestStatus::Failed => {
                        if let Some(error) = &request.error {
                            return Err(error.clone().into());
                        }
                    }
                    RequestStatus::Cancelled => {
                        return Err("Request was cancelled".into());
                    }
                    _ => {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    
    /// Cancel a network request
    pub async fn cancel(&self, request_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        
        // Cancel request in handlers
        for handler in handlers.values() {
            if let Err(e) = handler.cancel_request(request_id).await {
                warn!("Error cancelling request: {}", e);
            }
        }
        
        // Update request status
        let mut requests = self.requests.write().await;
        if let Some(request) = requests.get_mut(request_id) {
            request.status = RequestStatus::Cancelled;
            request.updated_at = Instant::now();
        }
        
        Ok(())
    }
    
    /// Run background task
    async fn run_background_task(&self, mut rx: mpsc::Receiver<BackgroundTask>) {
        let mut interval = tokio::time::interval(self.config.sync_interval);
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Process request queue
                    if let Err(e) = self.process_queue().await {
                        warn!("Error processing queue: {}", e);
                    }
                    
                    // Sync offline queue
                    if let Err(e) = self.sync_offline().await {
                        warn!("Error syncing offline queue: {}", e);
                    }
                    
                    // Clean up completed requests
                    if let Err(e) = self.cleanup_requests().await {
                        warn!("Error cleaning up requests: {}", e);
                    }
                }
                
                Some(task) = rx.recv() => {
                    match task {
                        BackgroundTask::ProcessQueue => {
                            if let Err(e) = self.process_queue().await {
                                warn!("Error processing queue: {}", e);
                            }
                        }
                        BackgroundTask::SyncOffline => {
                            if let Err(e) = self.sync_offline().await {
                                warn!("Error syncing offline queue: {}", e);
                            }
                        }
                        BackgroundTask::Cleanup => {
                            if let Err(e) = self.cleanup_requests().await {
                                warn!("Error cleaning up requests: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }
    
    /// Process request queue
    async fn process_queue(&self) -> Result<(), Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        let mut requests = self.requests.write().await;
        let mut queue = self.request_queue.write().await;
        
        while let Some(id) = queue.pop_front() {
            if let Some(request) = requests.get_mut(&id) {
                // Skip if max retries reached
                if request.retry_attempts >= self.config.max_retries {
                    request.status = RequestStatus::Failed;
                    request.error = Some("Max retry attempts reached".into());
                    continue;
                }
                
                // Try to execute request
                let mut success = false;
                for handler in handlers.values() {
                    match handler.execute_request(request).await {
                        Ok(response) => {
                            success = true;
                            request.status = RequestStatus::Completed;
                            request.response = Some(response);
                            break;
                        }
                        Err(e) => {
                            request.error = Some(e.to_string());
                        }
                    }
                }
                
                if !success {
                    request.retry_attempts += 1;
                    request.last_retry = Some(Instant::now());
                    queue.push_back(id);
                }
            }
        }
        
        Ok(())
    }
    
    /// Sync offline queue
    async fn sync_offline(&self) -> Result<(), Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        let mut requests = self.requests.write().await;
        let mut offline_queue = self.offline_queue.write().await;
        
        // Check connectivity
        let mut is_online = false;
        for handler in handlers.values() {
            if let Ok(online) = handler.check_connectivity().await {
                is_online = online;
                break;
            }
        }
        
        if !is_online {
            return Ok(());
        }
        
        // Process offline queue
        while let Some(id) = offline_queue.pop_front() {
            if let Some(request) = requests.get_mut(&id) {
                // Try to execute request
                let mut success = false;
                for handler in handlers.values() {
                    match handler.execute_request(request).await {
                        Ok(response) => {
                            success = true;
                            request.status = RequestStatus::Completed;
                            request.response = Some(response);
                            break;
                        }
                        Err(e) => {
                            request.error = Some(e.to_string());
                        }
                    }
                }
                
                if !success {
                    offline_queue.push_back(id);
                }
            }
        }
        
        Ok(())
    }
    
    /// Clean up completed requests
    async fn cleanup_requests(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut requests = self.requests.write().await;
        let cutoff = Instant::now() - Duration::from_secs(24 * 60 * 60); // 24 hours
        
        requests.retain(|_, request| {
            request.created_at > cutoff
        });
        
        Ok(())
    }
}

/// Example network handler implementation
pub struct ConsoleNetworkHandler;

#[async_trait::async_trait]
impl NetworkHandler for ConsoleNetworkHandler {
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing console network handler");
        Ok(())
    }
    
    async fn check_connectivity(&self) -> Result<bool, Box<dyn std::error::Error>> {
        info!("Checking console network connectivity");
        Ok(true)
    }
    
    async fn execute_request(&self, request: &NetworkRequest) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        info!("Executing console network request: {:?}", request);
        Ok(vec![])
    }
    
    async fn cancel_request(&self, request_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("Cancelling console network request: {}", request_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_network_operations() {
        let config = NetworkConfig::default();
        let manager = NetworkManager::new(config);
        
        // Register console handler
        manager.register_handler("console", ConsoleNetworkHandler).await;
        
        // Test request
        let request_config = RequestConfig {
            method: HttpMethod::Get,
            url: "https://api.example.com/test".to_string(),
            headers: HashMap::new(),
            body: None,
            timeout: Duration::from_secs(30),
            priority: RequestPriority::Normal,
            enable_cache: true,
            cache_ttl: None,
            enable_retry: true,
            max_retries: 3,
            retry_delay: Duration::from_secs(1),
        };
        
        // Execute request
        let result: Result<String, _> = manager.execute(request_config).await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_offline_mode() {
        let config = NetworkConfig::default();
        let manager = NetworkManager::new(config);
        
        // Register console handler
        manager.register_handler("console", ConsoleNetworkHandler).await;
        
        // Test request
        let request_config = RequestConfig {
            method: HttpMethod::Get,
            url: "https://api.example.com/test".to_string(),
            headers: HashMap::new(),
            body: None,
            timeout: Duration::from_secs(30),
            priority: RequestPriority::Normal,
            enable_cache: true,
            cache_ttl: None,
            enable_retry: true,
            max_retries: 3,
            retry_delay: Duration::from_secs(1),
        };
        
        // Execute request
        let result: Result<String, _> = manager.execute(request_config).await;
        assert!(result.is_err());
    }
} 