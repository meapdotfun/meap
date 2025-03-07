//! Rate limiting and request throttling for Deepseek API
//! Prevents API quota exhaustion and manages request flow

use crate::error::{DeepseekError, Result};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Rate limit configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per minute
    pub requests_per_minute: u32,
    /// Maximum requests per day
    pub requests_per_day: u32,
    /// Maximum tokens per minute
    pub tokens_per_minute: u32,
    /// Maximum parallel requests
    pub max_parallel_requests: u32,
    /// Retry backoff strategy
    pub retry_strategy: RetryStrategy,
    /// Whether to enable adaptive rate limiting
    pub adaptive_limiting: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,
            requests_per_day: 10000,
            tokens_per_minute: 100000,
            max_parallel_requests: 5,
            retry_strategy: RetryStrategy::default(),
            adaptive_limiting: true,
        }
    }
}

/// Retry backoff strategy
#[derive(Debug, Clone)]
pub struct RetryStrategy {
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub backoff_factor: f64,
    /// Maximum number of retries
    pub max_retries: u32,
    /// Whether to add jitter to retry timing
    pub add_jitter: bool,
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(60),
            backoff_factor: 1.5,
            max_retries: 5,
            add_jitter: true,
        }
    }
}

/// Request tracking information
#[derive(Debug, Clone)]
struct RequestInfo {
    /// When the request was made
    timestamp: Instant,
    /// Number of tokens in the request
    token_count: usize,
}

/// Rate limiter for API requests
pub struct RateLimiter {
    /// Configuration
    config: RateLimitConfig,
    /// Request history per minute
    minute_requests: Arc<RwLock<Vec<RequestInfo>>>,
    /// Request history per day
    day_requests: Arc<RwLock<Vec<RequestInfo>>>,
    /// Token usage per minute
    minute_tokens: Arc<RwLock<Vec<RequestInfo>>>,
    /// Active parallel requests
    active_requests: Arc<Mutex<u32>>,
    /// Last error timestamp
    last_error: Arc<RwLock<Option<(Instant, String)>>>,
    /// Current adaptive rate limit
    adaptive_limit: Arc<RwLock<u32>>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        let limiter = Self {
            config: config.clone(),
            minute_requests: Arc::new(RwLock::new(Vec::new())),
            day_requests: Arc::new(RwLock::new(Vec::new())),
            minute_tokens: Arc::new(RwLock::new(Vec::new())),
            active_requests: Arc::new(Mutex::new(0)),
            last_error: Arc::new(RwLock::new(None)),
            adaptive_limit: Arc::new(RwLock::new(config.requests_per_minute)),
        };
        
        // Start cleanup task
        limiter.start_cleanup_task();
        
        limiter
    }
    
    /// Start background cleanup task
    fn start_cleanup_task(&self) {
        let minute_requests = self.minute_requests.clone();
        let day_requests = self.day_requests.clone();
        let minute_tokens = self.minute_tokens.clone();
        let adaptive_limit = self.adaptive_limit.clone();
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                let now = Instant::now();
                
                // Clean up minute tracking
                {
                    let mut requests = minute_requests.write().await;
                    requests.retain(|req| now.duration_since(req.timestamp) < Duration::from_secs(60));
                }
                
                {
                    let mut tokens = minute_tokens.write().await;
                    tokens.retain(|req| now.duration_since(req.timestamp) < Duration::from_secs(60));
                }
                
                // Clean up day tracking
                {
                    let mut requests = day_requests.write().await;
                    requests.retain(|req| now.duration_since(req.timestamp) < Duration::from_secs(86400));
                }
                
                // Reset adaptive limit if needed
                if config.adaptive_limiting {
                    let mut limit = adaptive_limit.write().await;
                    if *limit < config.requests_per_minute {
                        *limit = (*limit as f64 * 1.1).min(config.requests_per_minute as f64) as u32;
                        debug!("Adaptive rate limit increased to {}", *limit);
                    }
                }
            }
        });
    }
    
    /// Check if a request can be made
    pub async fn check_request(&self, token_count: usize) -> Result<()> {
        // Check active parallel requests
        {
            let active = *self.active_requests.lock().await;
            if active >= self.config.max_parallel_requests {
                return Err(DeepseekError::RateLimitExceeded(
                    format!("Maximum parallel requests ({}) exceeded", self.config.max_parallel_requests)
                ));
            }
        }
        
        // Check minute request limit
        {
            let requests = self.minute_requests.read().await;
            let current_limit = if self.config.adaptive_limiting {
                *self.adaptive_limit.read().await
            } else {
                self.config.requests_per_minute
            };
            
            if requests.len() >= current_limit as usize {
                return Err(DeepseekError::RateLimitExceeded(
                    format!("Request limit per minute ({}) exceeded", current_limit)
                ));
            }
        }
        
        // Check day request limit
        {
            let requests = self.day_requests.read().await;
            if requests.len() >= self.config.requests_per_day as usize {
                return Err(DeepseekError::RateLimitExceeded(
                    format!("Request limit per day ({}) exceeded", self.config.requests_per_day)
                ));
            }
        }
        
        // Check token limit
        {
            let tokens = self.minute_tokens.read().await;
            let total_tokens: usize = tokens.iter().map(|t| t.token_count).sum();
            
            if total_tokens + token_count > self.config.tokens_per_minute as usize {
                return Err(DeepseekError::RateLimitExceeded(
                    format!("Token limit per minute ({}) exceeded", self.config.tokens_per_minute)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Record a successful request
    pub async fn record_request(&self, token_count: usize) {
        let now = Instant::now();
        let request = RequestInfo {
            timestamp: now,
            token_count,
        };
        
        // Update minute tracking
        {
            let mut requests = self.minute_requests.write().await;
            requests.push(request.clone());
        }
        
        // Update day tracking
        {
            let mut requests = self.day_requests.write().await;
            requests.push(request.clone());
        }
        
        // Update token tracking
        {
            let mut tokens = self.minute_tokens.write().await;
            tokens.push(request);
        }
    }
    
    /// Acquire a request slot
    pub async fn acquire(&self) -> Result<RequestGuard> {
        let mut active = self.active_requests.lock().await;
        *active += 1;
        
        Ok(RequestGuard {
            active_requests: self.active_requests.clone(),
        })
    }
    
    /// Record an API error
    pub async fn record_error(&self, error: &str) {
        let now = Instant::now();
        
        // Update last error
        {
            let mut last_error = self.last_error.write().await;
            *last_error = Some((now, error.to_string()));
        }
        
        // Reduce adaptive limit if enabled
        if self.config.adaptive_limiting {
            let mut limit = self.adaptive_limit.write().await;
            *limit = (*limit as f64 * 0.8).max(1.0) as u32;
            warn!("Adaptive rate limit decreased to {} after error", *limit);
        }
    }
    
    /// Calculate backoff duration for retry
    pub async fn calculate_backoff(&self, attempt: u32) -> Option<Duration> {
        if attempt >= self.config.retry_strategy.max_retries {
            return None;
        }
        
        let base_delay = self.config.retry_strategy.initial_delay.as_millis() as f64 * 
            self.config.retry_strategy.backoff_factor.powf(attempt as f64);
            
        let delay = base_delay.min(self.config.retry_strategy.max_delay.as_millis() as f64);
        
        let final_delay = if self.config.retry_strategy.add_jitter {
            // Add up to 25% jitter
            let jitter = rand::random::<f64>() * 0.25;
            delay * (1.0 + jitter)
        } else {
            delay
        };
        
        Some(Duration::from_millis(final_delay as u64))
    }
    
    /// Wait for the next retry with appropriate backoff
    pub async fn wait_for_retry(&self, attempt: u32) -> bool {
        if let Some(delay) = self.calculate_backoff(attempt).await {
            debug!("Retrying after {:?} (attempt {})", delay, attempt + 1);
            sleep(delay).await;
            true
        } else {
            false
        }
    }
    
    /// Get current rate limit statistics
    pub async fn get_stats(&self) -> RateLimitStats {
        let minute_requests = self.minute_requests.read().await.len() as u32;
        let day_requests = self.day_requests.read().await.len() as u32;
        let active_requests = *self.active_requests.lock().await;
        
        let minute_tokens: usize = self.minute_tokens.read().await
            .iter()
            .map(|t| t.token_count)
            .sum();
            
        let adaptive_limit = if self.config.adaptive_limiting {
            *self.adaptive_limit.read().await
        } else {
            self.config.requests_per_minute
        };
        
        RateLimitStats {
            minute_requests,
            day_requests,
            minute_tokens: minute_tokens as u32,
            active_requests,
            minute_limit: adaptive_limit,
            day_limit: self.config.requests_per_day,
            tokens_limit: self.config.tokens_per_minute,
            parallel_limit: self.config.max_parallel_requests,
        }
    }
}

/// RAII guard for active requests
pub struct RequestGuard {
    active_requests: Arc<Mutex<u32>>,
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        tokio::spawn(async move {
            let mut active = self.active_requests.lock().await;
            *active = active.saturating_sub(1);
        });
    }
}

/// Rate limit statistics
#[derive(Debug, Clone)]
pub struct RateLimitStats {
    /// Current requests in the last minute
    pub minute_requests: u32,
    /// Current requests in the last day
    pub day_requests: u32,
    /// Current tokens in the last minute
    pub minute_tokens: u32,
    /// Current active parallel requests
    pub active_requests: u32,
    /// Current minute request limit
    pub minute_limit: u32,
    /// Day request limit
    pub day_limit: u32,
    /// Token limit per minute
    pub tokens_limit: u32,
    /// Parallel request limit
    pub parallel_limit: u32,
}

/// Throttled client for rate-limited API access
pub struct ThrottledClient {
    /// Rate limiter
    rate_limiter: Arc<RateLimiter>,
    /// HTTP client
    client: reqwest::Client,
    /// Base URL
    base_url: String,
    /// API key
    api_key: String,
}

impl ThrottledClient {
    /// Create a new throttled client
    pub fn new(
        rate_limiter: Arc<RateLimiter>,
        base_url: String,
        api_key: String,
        timeout: Duration,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to create HTTP client");
            
        Self {
            rate_limiter,
            client,
            base_url,
            api_key,
        }
    }
    
    /// Send a request with rate limiting and retries
    pub async fn send_request<T: serde::Serialize + Clone>(
        &self,
        endpoint: &str,
        payload: &T,
        token_count: usize,
    ) -> Result<reqwest::Response> {
        // Check rate limits
        self.rate_limiter.check_request(token_count).await?;
        
        // Acquire request slot
        let _guard = self.rate_limiter.acquire().await?;
        
        // Record the request
        self.rate_limiter.record_request(token_count).await;
        
        // Try the request with retries
        let mut attempt = 0;
        loop {
            let url = format!("{}{}", self.base_url, endpoint);
            let result = self.client.post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(payload)
                .send()
                .await;
                
            match result {
                Ok(response) => {
                    if response.status().is_success() {
                        return Ok(response);
                    } else if response.status().as_u16() == 429 {
                        // Rate limit exceeded
                        let error = format!("Rate limit exceeded: {}", response.status());
                        self.rate_limiter.record_error(&error).await;
                        
                        if !self.rate_limiter.wait_for_retry(attempt).await {
                            return Err(DeepseekError::RateLimitExceeded(error));
                        }
                    } else {
                        // Other API error
                        let status = response.status();
                        let error_text = response.text().await
                            .unwrap_or_else(|_| "Unknown error".to_string());
                            
                        let error = format!("API error: {} - {}", status, error_text);
                        self.rate_limiter.record_error(&error).await;
                        
                        if !self.rate_limiter.wait_for_retry(attempt).await {
                            return Err(DeepseekError::ApiError(error));
                        }
                    }
                },
                Err(e) => {
                    // Network error
                    let error = format!("Network error: {}", e);
                    self.rate_limiter.record_error(&error).await;
                    
                    if !self.rate_limiter.wait_for_retry(attempt).await {
                        return Err(DeepseekError::NetworkError(error));
                    }
                }
            }
            
            attempt += 1;
        }
    }
    
    /// Get rate limit statistics
    pub async fn get_rate_limit_stats(&self) -> RateLimitStats {
        self.rate_limiter.get_stats().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_rate_limiter_basic() {
        let config = RateLimitConfig {
            requests_per_minute: 10,
            requests_per_day: 100,
            tokens_per_minute: 1000,
            max_parallel_requests: 3,
            retry_strategy: RetryStrategy::default(),
            adaptive_limiting: false,
        };
        
        let limiter = RateLimiter::new(config);
        
        // Check initial state
        let stats = limiter.get_stats().await;
        assert_eq!(stats.minute_requests, 0);
        assert_eq!(stats.day_requests, 0);
        assert_eq!(stats.minute_tokens, 0);
        assert_eq!(stats.active_requests, 0);
        
        // Record some requests
        for _ in 0..5 {
            limiter.check_request(100).await.unwrap();
            limiter.record_request(100).await;
        }
        
        // Check updated stats
        let stats = limiter.get_stats().await;
        assert_eq!(stats.minute_requests, 5);
        assert_eq!(stats.day_requests, 5);
        assert_eq!(stats.minute_tokens, 500);
        
        // Test parallel request limit
        let guard1 = limiter.acquire().await.unwrap();
        let guard2 = limiter.acquire().await.unwrap();
        let guard3 = limiter.acquire().await.unwrap();
        
        let stats = limiter.get_stats().await;
        assert_eq!(stats.active_requests, 3);
        
        // This should fail due to max parallel requests
        assert!(limiter.acquire().await.is_err());
        
        // Drop one guard
        drop(guard1);
        
        // Wait a bit for the async drop to complete
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        // Now we should be able to acquire again
        let _guard4 = limiter.acquire().await.unwrap();
        
        // Clean up
        drop(guard2);
        drop(guard3);
        drop(_guard4);
    }
    
    #[tokio::test]
    async fn test_rate_limiter_limits() {
        let config = RateLimitConfig {
            requests_per_minute: 5,
            requests_per_day: 100,
            tokens_per_minute: 500,
            max_parallel_requests: 3,
            retry_strategy: RetryStrategy::default(),
            adaptive_limiting: false,
        };
        
        let limiter = RateLimiter::new(config);
        
        // Fill up to the limit
        for _ in 0..5 {
            limiter.check_request(100).await.unwrap();
            limiter.record_request(100).await;
        }
        
        // This should fail due to minute request limit
        assert!(limiter.check_request(100).await.is_err());
        
        // This should fail due to token limit
        assert!(limiter.check_request(100).await.is_err());
    }
    
    #[tokio::test]
    async fn test_backoff_calculation() {
        let config = RateLimitConfig {
            requests_per_minute: 10,
            requests_per_day: 100,
            tokens_per_minute: 1000,
            max_parallel_requests: 3,
            retry_strategy: RetryStrategy {
                initial_delay: Duration::from_millis(100),
                max_delay: Duration::from_secs(10),
                backoff_factor: 2.0,
                max_retries: 3,
                add_jitter: false,
            },
            adaptive_limiting: false,
        };
        
        let limiter = RateLimiter::new(config);
        
        // Test backoff progression
        let backoff0 = limiter.calculate_backoff(0).await.unwrap();
        let backoff1 = limiter.calculate_backoff(1).await.unwrap();
        let backoff2 = limiter.calculate_backoff(2).await.unwrap();
        
        // Should be exponential
        assert!(backoff1 > backoff0);
        assert!(backoff2 > backoff1);
        
        // Should respect max retries
        assert!(limiter.calculate_backoff(3).await.is_none());
    }
} 