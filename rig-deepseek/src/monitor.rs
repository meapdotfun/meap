//! Health monitoring and diagnostics for Deepseek models
//! Tracks model health, performance, and resource usage

use crate::error::Result;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Health status of a model
#[derive(Debug, Clone, PartialEq)]
pub enum ModelHealth {
    /// Model is operating normally
    Healthy,
    /// Model is experiencing issues but still functional
    Degraded(String),
    /// Model is not functioning
    Unhealthy(String),
}

/// Model performance metrics
#[derive(Debug, Clone)]
pub struct ModelStats {
    /// Requests per second
    pub requests_per_sec: f64,
    /// Average response time
    pub avg_response_time: Duration,
    /// Error rate percentage
    pub error_rate: f64,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// GPU utilization percentage
    pub gpu_utilization: f64,
}

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// How often to run health checks
    pub check_interval: Duration,
    /// Maximum allowed error rate
    pub max_error_rate: f64,
    /// Maximum allowed response time
    pub max_response_time: Duration,
    /// Maximum allowed memory usage
    pub max_memory_usage: u64,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(60),
            max_error_rate: 0.05, // 5%
            max_response_time: Duration::from_secs(5),
            max_memory_usage: 8 * 1024 * 1024 * 1024, // 8GB
        }
    }
}

/// Monitors model health and performance
pub struct ModelMonitor {
    config: HealthConfig,
    health_states: Arc<RwLock<HashMap<String, ModelHealth>>>,
    stats: Arc<RwLock<HashMap<String, ModelStats>>>,
    last_check: Arc<RwLock<Instant>>,
}

impl ModelMonitor {
    pub fn new(config: HealthConfig) -> Self {
        let monitor = Self {
            config,
            health_states: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(HashMap::new())),
            last_check: Arc::new(RwLock::new(Instant::now())),
        };

        // Start background health check task
        monitor.start_health_checks();

        monitor
    }

    /// Starts periodic health checks
    fn start_health_checks(&self) {
        let health_states = self.health_states.clone();
        let stats = self.stats.clone();
        let config = self.config.clone();
        let last_check = self.last_check.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.check_interval);
            loop {
                interval.tick().await;
                
                // Update last check time
                *last_check.write().await = Instant::now();

                // Check each model's health
                let mut states = health_states.write().await;
                let stats = stats.read().await;

                for (model_id, model_stats) in stats.iter() {
                    let health = if model_stats.error_rate > config.max_error_rate {
                        ModelHealth::Unhealthy(format!("Error rate too high: {:.2}%", 
                            model_stats.error_rate * 100.0))
                    } else if model_stats.avg_response_time > config.max_response_time {
                        ModelHealth::Degraded(format!("Response time too high: {:?}", 
                            model_stats.avg_response_time))
                    } else if model_stats.memory_usage > config.max_memory_usage {
                        ModelHealth::Degraded(format!("High memory usage: {} bytes", 
                            model_stats.memory_usage))
                    } else {
                        ModelHealth::Healthy
                    };

                    if let Some(old_health) = states.get(model_id) {
                        if *old_health != health {
                            match &health {
                                ModelHealth::Healthy => info!("Model {} is now healthy", model_id),
                                ModelHealth::Degraded(reason) => warn!("Model {} is degraded: {}", 
                                    model_id, reason),
                                ModelHealth::Unhealthy(reason) => error!("Model {} is unhealthy: {}", 
                                    model_id, reason),
                            }
                        }
                    }

                    states.insert(model_id.clone(), health);
                }
            }
        });
    }

    /// Updates stats for a model
    pub async fn update_stats(&self, model_id: &str, stats: ModelStats) {
        let mut model_stats = self.stats.write().await;
        model_stats.insert(model_id.to_string(), stats);
    }

    /// Gets current health status of a model
    pub async fn get_health(&self, model_id: &str) -> Option<ModelHealth> {
        let states = self.health_states.read().await;
        states.get(model_id).cloned()
    }

    /// Gets current stats for a model
    pub async fn get_stats(&self, model_id: &str) -> Option<ModelStats> {
        let stats = self.stats.read().await;
        stats.get(model_id).cloned()
    }

    /// Gets time since last health check
    pub async fn time_since_last_check(&self) -> Duration {
        let last_check = self.last_check.read().await;
        last_check.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_monitoring() {
        let config = HealthConfig {
            check_interval: Duration::from_millis(100),
            max_error_rate: 0.05,
            max_response_time: Duration::from_secs(1),
            max_memory_usage: 1024 * 1024 * 1024,
        };

        let monitor = ModelMonitor::new(config);

        // Test healthy state
        monitor.update_stats("model1", ModelStats {
            requests_per_sec: 10.0,
            avg_response_time: Duration::from_millis(100),
            error_rate: 0.01,
            memory_usage: 512 * 1024 * 1024,
            gpu_utilization: 0.5,
        }).await;

        tokio::time::sleep(Duration::from_millis(200)).await;

        let health = monitor.get_health("model1").await.unwrap();
        assert!(matches!(health, ModelHealth::Healthy));

        // Test degraded state
        monitor.update_stats("model1", ModelStats {
            requests_per_sec: 10.0,
            avg_response_time: Duration::from_secs(2),
            error_rate: 0.01,
            memory_usage: 512 * 1024 * 1024,
            gpu_utilization: 0.5,
        }).await;

        tokio::time::sleep(Duration::from_millis(200)).await;

        let health = monitor.get_health("model1").await.unwrap();
        assert!(matches!(health, ModelHealth::Degraded(_)));
    }
} 