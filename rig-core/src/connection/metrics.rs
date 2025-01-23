//! Connection metrics tracking and monitoring

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Tracks various metrics for a connection
#[derive(Debug)]
pub struct ConnectionMetrics {
    /// Total messages sent
    messages_sent: AtomicU64,
    /// Total messages received
    messages_received: AtomicU64,
    /// Total errors encountered
    errors: AtomicU64,
    /// Last active timestamp
    last_active: Arc<RwLock<Instant>>,
    /// Average message latency
    latency: Arc<RwLock<Duration>>,
}

impl ConnectionMetrics {
    pub fn new() -> Self {
        Self {
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            last_active: Arc::new(RwLock::new(Instant::now())),
            latency: Arc::new(RwLock::new(Duration::default())),
        }
    }

    pub fn record_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.update_last_active();
    }

    pub fn record_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
        self.update_last_active();
    }

    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_latency(&self, duration: Duration) {
        let mut latency = self.latency.write().blocking_lock();
        *latency = duration;
    }

    fn update_last_active(&self) {
        let mut last_active = self.last_active.write().blocking_lock();
        *last_active = Instant::now();
    }

    pub fn get_metrics(&self) -> ConnectionStats {
        ConnectionStats {
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            last_active: *self.last_active.read().blocking_lock(),
            latency: *self.latency.read().blocking_lock(),
        }
    }
}

/// Connection statistics at a point in time
#[derive(Debug, Clone, Copy)]
pub struct ConnectionStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub errors: u64,
    pub last_active: Instant,
    pub latency: Duration,
} 