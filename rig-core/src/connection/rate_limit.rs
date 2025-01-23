//! Rate limiting implementation for connection management

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use crate::error::{Error, Result};

/// Rate limiting configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum number of requests per window
    pub max_requests: u32,
    /// Time window for rate limiting
    pub window_size: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_size: Duration::from_secs(60),
        }
    }
}

/// Rate limiter for managing request rates
pub struct RateLimiter {
    config: RateLimitConfig,
    requests: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            requests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Checks if a request should be allowed
    pub async fn check_request(&self, client_id: &str) -> Result<()> {
        let now = Instant::now();
        let mut requests = self.requests.write().await;
        
        // Get or create request history for client
        let history = requests.entry(client_id.to_string())
            .or_insert_with(Vec::new);

        // Remove expired requests
        history.retain(|&time| now.duration_since(time) < self.config.window_size);

        // Check if under limit
        if history.len() >= self.config.max_requests as usize {
            return Err(Error::RateLimit(format!(
                "Rate limit exceeded for client {}. Maximum {} requests per {:?}",
                client_id,
                self.config.max_requests,
                self.config.window_size
            )));
        }

        // Add new request
        history.push(now);
        Ok(())
    }

    /// Cleans up expired request history
    pub async fn cleanup(&self) {
        let now = Instant::now();
        let mut requests = self.requests.write().await;
        
        requests.retain(|_, history| {
            history.retain(|&time| now.duration_since(time) < self.config.window_size);
            !history.is_empty()
        });
    }
} 