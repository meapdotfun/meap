use crate::error::Result;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use std::sync::Arc;
use std::collections::VecDeque;

/// Tracks performance metrics over time windows
pub struct PerformanceTracker {
    /// Latency measurements in microseconds
    latencies: Arc<RwLock<VecDeque<u64>>>,
    /// Messages per second
    throughput: Arc<RwLock<VecDeque<u64>>>,
    /// Window size for measurements
    window_size: Duration,
    /// Last measurement timestamp
    last_update: Arc<RwLock<Instant>>,
}

impl PerformanceTracker {
    pub fn new(window_size: Duration) -> Self {
        Self {
            latencies: Arc::new(RwLock::new(VecDeque::new())),
            throughput: Arc::new(RwLock::new(VecDeque::new())), 
            window_size,
            last_update: Arc::new(RwLock::new(Instant::now())),
        }
    }

    /// Records a new latency measurement
    pub async fn record_latency(&self, latency: Duration) {
        let mut latencies = self.latencies.write().await;
        latencies.push_back(latency.as_micros() as u64);
        
        // Keep only measurements within window
        while latencies.len() > 1000 {
            latencies.pop_front();
        }
    }

    /// Records messages processed in last interval
    pub async fn record_throughput(&self, count: u64) {
        let mut throughput = self.throughput.write().await;
        throughput.push_back(count);
        
        while throughput.len() > 100 {
            throughput.pop_front(); 
        }
    }

    /// Gets current performance metrics
    pub async fn get_metrics(&self) -> PerfMetrics {
        let latencies = self.latencies.read().await;
        let throughput = self.throughput.read().await;
        
        let avg_latency = latencies.iter().sum::<u64>() as f64 / latencies.len() as f64;
        let max_latency = latencies.iter().max().copied().unwrap_or(0);
        let p99_latency = self.percentile(&latencies, 0.99);
        
        let avg_throughput = throughput.iter().sum::<u64>() as f64 / throughput.len() as f64;
        let max_throughput = throughput.iter().max().copied().unwrap_or(0);

        PerfMetrics {
            avg_latency_us: avg_latency,
            max_latency_us: max_latency,
            p99_latency_us: p99_latency,
            avg_throughput: avg_throughput,
            max_throughput,
        }
    }

    fn percentile(&self, values: &VecDeque<u64>, percentile: f64) -> u64 {
        let mut sorted = values.iter().copied().collect::<Vec<_>>();
        sorted.sort_unstable();
        
        let index = (sorted.len() as f64 * percentile) as usize;
        sorted.get(index).copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone)]
pub struct PerfMetrics {
    pub avg_latency_us: f64,
    pub max_latency_us: u64, 
    pub p99_latency_us: u64,
    pub avg_throughput: f64,
    pub max_throughput: u64,
} 