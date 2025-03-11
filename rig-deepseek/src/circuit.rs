//! Circuit breaker pattern implementation for Deepseek API
//! Prevents cascading failures and provides graceful degradation

use crate::error::{DeepseekError, Result};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed, requests flow normally
    Closed,
    /// Circuit is open, requests are blocked
    Open,
    /// Circuit is half-open, allowing test requests
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "Closed"),
            CircuitState::Open => write!(f, "Open"),
            CircuitState::HalfOpen => write!(f, "Half-Open"),
        }
    }
}

/// Configuration for circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Failure threshold to trip the circuit
    pub failure_threshold: u32,
    /// Success threshold to reset the circuit
    pub success_threshold: u32,
    /// Timeout before attempting to half-open the circuit
    pub reset_timeout: Duration,
    /// Maximum number of requests allowed in half-open state
    pub half_open_max_requests: u32,
    /// Window size for failure counting
    pub failure_window: Duration,
    /// Whether to enable automatic recovery
    pub auto_recovery: bool,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            reset_timeout: Duration::from_secs(30),
            half_open_max_requests: 1,
            failure_window: Duration::from_secs(60),
            auto_recovery: true,
        }
    }
}

/// Failure tracking information
#[derive(Debug, Clone)]
struct FailureInfo {
    /// When the failure occurred
    timestamp: Instant,
    /// Error message
    error: String,
}

/// Circuit breaker for API resilience
pub struct CircuitBreaker {
    /// Name of this circuit breaker
    name: String,
    /// Configuration
    config: CircuitBreakerConfig,
    /// Current state
    state: Arc<RwLock<CircuitState>>,
    /// Recent failures
    failures: Arc<RwLock<Vec<FailureInfo>>>,
    /// Consecutive successes in half-open state
    half_open_successes: Arc<RwLock<u32>>,
    /// Current requests in half-open state
    half_open_requests: Arc<RwLock<u32>>,
    /// When the circuit was last opened
    last_opened: Arc<RwLock<Option<Instant>>>,
    /// When the circuit was last reset
    last_reset: Arc<RwLock<Option<Instant>>>,
    /// Total number of times the circuit has tripped
    trip_count: Arc<RwLock<u32>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(name: &str, config: CircuitBreakerConfig) -> Self {
        let breaker = Self {
            name: name.to_string(),
            config,
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failures: Arc::new(RwLock::new(Vec::new())),
            half_open_successes: Arc::new(RwLock::new(0)),
            half_open_requests: Arc::new(RwLock::new(0)),
            last_opened: Arc::new(RwLock::new(None)),
            last_reset: Arc::new(RwLock::new(None)),
            trip_count: Arc::new(RwLock::new(0)),
        };
        
        // Start recovery task if auto-recovery is enabled
        if config.auto_recovery {
            breaker.start_recovery_task();
        }
        
        breaker
    }
    
    /// Start background recovery task
    fn start_recovery_task(&self) {
        let state = self.state.clone();
        let last_opened = self.last_opened.clone();
        let half_open_requests = self.half_open_requests.clone();
        let reset_timeout = self.config.reset_timeout;
        let name = self.name.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            
            loop {
                interval.tick().await;
                
                // Check if circuit is open
                let current_state = *state.read().await;
                if current_state == CircuitState::Open {
                    // Check if reset timeout has elapsed
                    if let Some(opened_time) = *last_opened.read().await {
                        if opened_time.elapsed() >= reset_timeout {
                            // Transition to half-open
                            debug!("Circuit '{}' transitioning to half-open state after timeout", name);
                            *state.write().await = CircuitState::HalfOpen;
                            *half_open_requests.write().await = 0;
                        }
                    }
                }
            }
        });
    }
    
    /// Check if a request is allowed
    pub async fn allow_request(&self) -> bool {
        let state = *self.state.read().await;
        
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => false,
            CircuitState::HalfOpen => {
                let mut requests = self.half_open_requests.write().await;
                if *requests < self.config.half_open_max_requests {
                    *requests += 1;
                    true
                } else {
                    false
                }
            }
        }
    }
    
    /// Execute a function with circuit breaker protection
    pub async fn execute<F, T, E>(&self, f: F) -> Result<T>
    where
        F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<T, E>> + Send>>,
        E: std::fmt::Display,
    {
        if !self.allow_request().await {
            return Err(DeepseekError::CircuitOpen(format!(
                "Circuit '{}' is open, request rejected", self.name
            )));
        }
        
        let current_state = *self.state.read().await;
        let result = f().await;
        
        match result {
            Ok(value) => {
                // Record success
                if current_state == CircuitState::HalfOpen {
                    self.record_success().await;
                }
                Ok(value)
            }
            Err(e) => {
                // Record failure
                self.record_failure(&e.to_string()).await;
                Err(DeepseekError::ExternalService(format!(
                    "Service error: {}", e
                )))
            }
        }
    }
    
    /// Record a successful request
    pub async fn record_success(&self) {
        let current_state = *self.state.read().await;
        
        if current_state == CircuitState::HalfOpen {
            let mut successes = self.half_open_successes.write().await;
            *successes += 1;
            
            if *successes >= self.config.success_threshold {
                // Reset circuit
                debug!("Circuit '{}' reset after {} consecutive successes", 
                    self.name, *successes);
                    
                *self.state.write().await = CircuitState::Closed;
                *successes = 0;
                *self.last_reset.write().await = Some(Instant::now());
                
                // Clear failure history
                self.failures.write().await.clear();
            }
        }
    }
    
    /// Record a failed request
    pub async fn record_failure(&self, error: &str) {
        let now = Instant::now();
        let current_state = *self.state.read().await;
        
        // Add to failure history
        {
            let mut failures = self.failures.write().await;
            
            // Remove old failures outside the window
            failures.retain(|f| now.duration_since(f.timestamp) < self.config.failure_window);
            
            // Add new failure
            failures.push(FailureInfo {
                timestamp: now,
                error: error.to_string(),
            });
            
            // Check if threshold is reached in closed state
            if current_state == CircuitState::Closed && failures.len() >= self.config.failure_threshold as usize {
                self.trip_circuit().await;
            }
        }
        
        // If in half-open state, any failure trips the circuit again
        if current_state == CircuitState::HalfOpen {
            warn!("Circuit '{}' tripped again in half-open state: {}", self.name, error);
            self.trip_circuit().await;
        }
    }
    
    /// Trip the circuit breaker
    async fn trip_circuit(&self) {
        let mut state = self.state.write().await;
        if *state != CircuitState::Open {
            *state = CircuitState::Open;
            *self.last_opened.write().await = Some(Instant::now());
            *self.trip_count.write().await += 1;
            
            warn!("Circuit '{}' tripped open (count: {})", 
                self.name, *self.trip_count.read().await);
        }
    }
    
    /// Manually reset the circuit breaker
    pub async fn reset(&self) {
        let mut state = self.state.write().await;
        *state = CircuitState::Closed;
        *self.half_open_successes.write().await = 0;
        *self.half_open_requests.write().await = 0;
        *self.last_reset.write().await = Some(Instant::now());
        
        self.failures.write().await.clear();
        
        info!("Circuit '{}' manually reset", self.name);
    }
    
    /// Get the current state
    pub async fn get_state(&self) -> CircuitState {
        *self.state.read().await
    }
    
    /// Get circuit breaker statistics
    pub async fn get_stats(&self) -> CircuitBreakerStats {
        let state = *self.state.read().await;
        let failures = self.failures.read().await;
        let trip_count = *self.trip_count.read().await;
        let half_open_successes = *self.half_open_successes.read().await;
        
        let time_in_current_state = match state {
            CircuitState::Open => {
                if let Some(opened_time) = *self.last_opened.read().await {
                    opened_time.elapsed()
                } else {
                    Duration::from_secs(0)
                }
            },
            CircuitState::Closed => {
                if let Some(reset_time) = *self.last_reset.read().await {
                    reset_time.elapsed()
                } else {
                    Duration::from_secs(0)
                }
            },
            CircuitState::HalfOpen => {
                if let Some(opened_time) = *self.last_opened.read().await {
                    opened_time.elapsed()
                } else {
                    Duration::from_secs(0)
                }
            },
        };
        
        CircuitBreakerStats {
            name: self.name.clone(),
            state,
            failure_count: failures.len() as u32,
            trip_count,
            half_open_successes,
            time_in_current_state,
            last_failure: failures.last().map(|f| f.error.clone()),
            last_failure_time: failures.last().map(|f| f.timestamp),
        }
    }
}

/// Circuit breaker statistics
#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    /// Circuit breaker name
    pub name: String,
    /// Current state
    pub state: CircuitState,
    /// Number of recent failures
    pub failure_count: u32,
    /// Total number of times the circuit has tripped
    pub trip_count: u32,
    /// Consecutive successes in half-open state
    pub half_open_successes: u32,
    /// Time in current state
    pub time_in_current_state: Duration,
    /// Last failure message
    pub last_failure: Option<String>,
    /// Last failure time
    pub last_failure_time: Option<Instant>,
}

/// Circuit breaker registry for managing multiple breakers
pub struct CircuitBreakerRegistry {
    /// Registered circuit breakers
    breakers: Arc<RwLock<HashMap<String, Arc<CircuitBreaker>>>>,
}

impl CircuitBreakerRegistry {
    /// Create a new circuit breaker registry
    pub fn new() -> Self {
        Self {
            breakers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Register a circuit breaker
    pub async fn register(&self, breaker: Arc<CircuitBreaker>) -> Result<()> {
        let name = breaker.name.clone();
        let mut breakers = self.breakers.write().await;
        
        if breakers.contains_key(&name) {
            return Err(DeepseekError::InvalidRequest(format!(
                "Circuit breaker '{}' already registered", name
            )));
        }
        
        breakers.insert(name, breaker);
        Ok(())
    }
    
    /// Get a circuit breaker by name
    pub async fn get(&self, name: &str) -> Option<Arc<CircuitBreaker>> {
        let breakers = self.breakers.read().await;
        breakers.get(name).cloned()
    }
    
    /// Get or create a circuit breaker
    pub async fn get_or_create(&self, name: &str, config: CircuitBreakerConfig) -> Arc<CircuitBreaker> {
        let breakers = self.breakers.read().await;
        
        if let Some(breaker) = breakers.get(name) {
            return breaker.clone();
        }
        
        drop(breakers);
        
        let breaker = Arc::new(CircuitBreaker::new(name, config));
        let mut breakers = self.breakers.write().await;
        
        // Check again in case another thread created it
        if let Some(existing) = breakers.get(name) {
            return existing.clone();
        }
        
        breakers.insert(name.to_string(), breaker.clone());
        breaker
    }
    
    /// List all circuit breakers
    pub async fn list(&self) -> Vec<String> {
        let breakers = self.breakers.read().await;
        breakers.keys().cloned().collect()
    }
    
    /// Get statistics for all circuit breakers
    pub async fn get_all_stats(&self) -> Vec<CircuitBreakerStats> {
        let breakers = self.breakers.read().await;
        let mut stats = Vec::with_capacity(breakers.len());
        
        for breaker in breakers.values() {
            stats.push(breaker.get_stats().await);
        }
        
        stats
    }
    
    /// Reset all circuit breakers
    pub async fn reset_all(&self) {
        let breakers = self.breakers.read().await;
        
        for breaker in breakers.values() {
            breaker.reset().await;
        }
        
        info!("Reset all circuit breakers");
    }
}

/// Wrapper for a service with circuit breaker protection
pub struct CircuitProtectedService<T> {
    /// The underlying service
    service: T,
    /// Circuit breaker
    circuit_breaker: Arc<CircuitBreaker>,
}

impl<T> CircuitProtectedService<T> {
    /// Create a new circuit-protected service
    pub fn new(service: T, circuit_breaker: Arc<CircuitBreaker>) -> Self {
        Self {
            service,
            circuit_breaker,
        }
    }
    
    /// Get a reference to the underlying service
    pub fn service(&self) -> &T {
        &self.service
    }
    
    /// Get a mutable reference to the underlying service
    pub fn service_mut(&mut self) -> &mut T {
        &mut self.service
    }
    
    /// Get the circuit breaker
    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }
    
    /// Execute a method with circuit breaker protection
    pub async fn execute<F, R, E>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&T) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<R, E>> + Send>>,
        E: std::fmt::Display,
    {
        self.circuit_breaker.execute(|| Box::pin(f(&self.service))).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    
    #[tokio::test]
    async fn test_circuit_breaker_basic() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            reset_timeout: Duration::from_millis(100),
            half_open_max_requests: 1,
            failure_window: Duration::from_secs(60),
            auto_recovery: true,
        };
        
        let breaker = CircuitBreaker::new("test", config);
        
        // Initially closed
        assert_eq!(breaker.get_state().await, CircuitState::Closed);
        assert!(breaker.allow_request().await);
        
        // Record failures to trip the circuit
        for _ in 0..3 {
            breaker.record_failure("test error").await;
        }
        
        // Should be open now
        assert_eq!(breaker.get_state().await, CircuitState::Open);
        assert!(!breaker.allow_request().await);
        
        // Wait for reset timeout
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Should be half-open now
        assert_eq!(breaker.get_state().await, CircuitState::HalfOpen);
        
        // One request allowed in half-open
        assert!(breaker.allow_request().await);
        assert!(!breaker.allow_request().await); // Second request blocked
        
        // Record successes to close the circuit
        breaker.record_success().await;
        breaker.record_success().await;
        
        // Should be closed again
        assert_eq!(breaker.get_state().await, CircuitState::Closed);
        assert!(breaker.allow_request().await);
    }
    
    #[tokio::test]
    async fn test_circuit_breaker_execute() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            reset_timeout: Duration::from_millis(100),
            half_open_max_requests: 1,
            failure_window: Duration::from_secs(60),
            auto_recovery: true,
        };
        
        let breaker = CircuitBreaker::new("test-execute", config);
        
        // Successful execution
        let result = breaker.execute(|| Box::pin(async {
            Ok::<_, String>("success")
        })).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        
        // Failed executions
        for _ in 0..3 {
            let result = breaker.execute(|| Box::pin(async {
                Err::<String, _>("test error")
            })).await;
            
            assert!(result.is_err());
        }
        
        // Circuit should be open now
        assert_eq!(breaker.get_state().await, CircuitState::Open);
        
        // Execution should fail immediately
        let result = breaker.execute(|| Box::pin(async {
            Ok::<_, String>("should not execute")
        })).await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DeepseekError::CircuitOpen(_)));
    }
    
    #[tokio::test]
    async fn test_protected_service() {
        // Simple service that counts calls
        struct TestService {
            success_calls: AtomicU32,
            error_calls: AtomicU32,
        }
        
        impl TestService {
            fn new() -> Self {
                Self {
                    success_calls: AtomicU32::new(0),
                    error_calls: AtomicU32::new(0),
                }
            }
            
            async fn call_success(&self) -> Result<u32, String> {
                let count = self.success_calls.fetch_add(1, Ordering::SeqCst) + 1;
                Ok(count)
            }
            
            async fn call_error(&self) -> Result<u32, String> {
                let count = self.error_calls.fetch_add(1, Ordering::SeqCst) + 1;
                Err(format!("Error {}", count))
            }
        }
        
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            reset_timeout: Duration::from_millis(100),
            half_open_max_requests: 1,
            failure_window: Duration::from_secs(60),
            auto_recovery: true,
        };
        
        let breaker = Arc::new(CircuitBreaker::new("test-service", config));
        let service = TestService::new();
        let protected = CircuitProtectedService::new(service, breaker);
        
        // Successful calls
        let result = protected.execute(|svc| Box::pin(svc.call_success())).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
        
        // Error calls to trip the circuit
        for i in 1..=3 {
            let result = protected.execute(|svc| Box::pin(svc.call_error())).await;
            assert!(result.is_err());
            assert_eq!(protected.service().error_calls.load(Ordering::SeqCst), i);
        }
        
        // Circuit should be open now
        assert_eq!(protected.circuit_breaker().get_state().await, CircuitState::Open);
        
        // Call should fail without executing service
        let result = protected.execute(|svc| Box::pin(svc.call_success())).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DeepseekError::CircuitOpen(_)));
        assert_eq!(protected.service().success_calls.load(Ordering::SeqCst), 1); // Still 1
        
        // Wait for reset timeout
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Should be half-open now, allowing one request
        assert_eq!(protected.circuit_breaker().get_state().await, CircuitState::HalfOpen);
        
        // Successful call in half-open state
        let result = protected.execute(|svc| Box::pin(svc.call_success())).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);
        
        // Need one more success to close the circuit
        protected.circuit_breaker().record_success().await;
        
        // Circuit should be closed again
        assert_eq!(protected.circuit_breaker().get_state().await, CircuitState::Closed);
    }
} 