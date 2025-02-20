//! Metrics collection and monitoring for Deepseek models
//! Tracks performance, usage, and error rates

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Metrics for a single model
#[derive(Debug, Clone)]
pub struct ModelMetrics {
    /// Total number of requests
    pub requests: u64,
    /// Number of successful requests
    pub successes: u64,
    /// Number of failed requests
    pub failures: u64,
    /// Total tokens generated/processed
    pub tokens: u64,
    /// Average latency
    pub avg_latency: Duration,
    /// Last request timestamp
    pub last_request: Instant,
}

impl Default for ModelMetrics {
    fn default() -> Self {
        Self {
            requests: 0,
            successes: 0,
            failures: 0,
            tokens: 0,
            avg_latency: Duration::default(),
            last_request: Instant::now(),
        }
    }
}

/// Metrics collector for Deepseek models
pub struct MetricsCollector {
    /// Metrics per model
    metrics: Arc<RwLock<HashMap<String, ModelMetrics>>>,
    /// Rolling window size for averages
    window_size: Duration,
}

impl MetricsCollector {
    pub fn new(window_size: Duration) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            window_size,
        }
    }

    /// Records a successful request
    pub async fn record_success(
        &self,
        model: &str,
        tokens: u64,
        latency: Duration,
    ) {
        let mut metrics = self.metrics.write().await;
        let entry = metrics.entry(model.to_string())
            .or_default();

        entry.requests += 1;
        entry.successes += 1;
        entry.tokens += tokens;
        entry.last_request = Instant::now();

        // Update rolling average latency
        entry.avg_latency = Duration::from_secs_f64(
            (entry.avg_latency.as_secs_f64() * (entry.requests - 1) as f64 +
             latency.as_secs_f64()) / entry.requests as f64
        );
    }

    /// Records a failed request
    pub async fn record_failure(&self, model: &str) {
        let mut metrics = self.metrics.write().await;
        let entry = metrics.entry(model.to_string())
            .or_default();

        entry.requests += 1;
        entry.failures += 1;
        entry.last_request = Instant::now();
    }

    /// Gets metrics for a specific model
    pub async fn get_metrics(&self, model: &str) -> Option<ModelMetrics> {
        let metrics = self.metrics.read().await;
        metrics.get(model).cloned()
    }

    /// Gets aggregated metrics across all models
    pub async fn get_aggregated_metrics(&self) -> AggregatedMetrics {
        let metrics = self.metrics.read().await;
        let mut aggregated = AggregatedMetrics::default();

        for (_, model_metrics) in metrics.iter() {
            aggregated.total_requests += model_metrics.requests;
            aggregated.total_successes += model_metrics.successes;
            aggregated.total_failures += model_metrics.failures;
            aggregated.total_tokens += model_metrics.tokens;
        }

        aggregated.model_count = metrics.len();
        aggregated
    }

    /// Removes stale metrics outside the window
    pub async fn cleanup_stale_metrics(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.retain(|_, m| m.last_request.elapsed() < self.window_size);
        debug!("Cleaned up stale metrics, remaining models: {}", metrics.len());
    }
}

/// Aggregated metrics across all models
#[derive(Debug, Default)]
pub struct AggregatedMetrics {
    /// Total number of models
    pub model_count: usize,
    /// Total requests across all models
    pub total_requests: u64,
    /// Total successful requests
    pub total_successes: u64,
    /// Total failed requests
    pub total_failures: u64,
    /// Total tokens processed
    pub total_tokens: u64,
}

impl AggregatedMetrics {
    /// Calculates success rate as percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            (self.total_successes as f64 / self.total_requests as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collection() {
        let collector = MetricsCollector::new(Duration::from_secs(60));

        // Record some metrics
        collector.record_success(
            "test-model",
            100,
            Duration::from_millis(500)
        ).await;

        collector.record_failure("test-model").await;

        // Check individual metrics
        let metrics = collector.get_metrics("test-model").await.unwrap();
        assert_eq!(metrics.requests, 2);
        assert_eq!(metrics.successes, 1);
        assert_eq!(metrics.failures, 1);
        assert_eq!(metrics.tokens, 100);

        // Check aggregated metrics
        let aggregated = collector.get_aggregated_metrics().await;
        assert_eq!(aggregated.model_count, 1);
        assert_eq!(aggregated.total_requests, 2);
        assert_eq!(aggregated.success_rate(), 50.0);
    }
} 