use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use crate::error::Result;

/// Memory allocation tracking
#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    /// Total heap size in bytes
    heap_size: usize,
    /// Memory used by different components
    component_usage: HashMap<String, usize>,
    /// Number of active allocations
    allocation_count: usize,
    /// Timestamp of snapshot
    timestamp: Instant,
}

/// Memory leak detection configuration  
#[derive(Debug, Clone)]
pub struct LeakConfig {
    /// Growth threshold that triggers warning
    growth_threshold: f64,
    /// Minimum time between checks
    check_interval: Duration,
    /// Components to monitor
    monitored_components: Vec<String>,
}

/// Memory profiler for tracking allocations and detecting leaks
pub struct MemoryProfiler {
    /// Historical memory snapshots
    snapshots: Arc<RwLock<Vec<MemorySnapshot>>>,
    /// Active allocations by component
    allocations: Arc<RwLock<HashMap<String, Vec<usize>>>>,
    /// Leak detection config
    leak_config: LeakConfig,
    /// Last leak check time
    last_check: Arc<RwLock<Instant>>,
}

impl MemoryProfiler {
    pub fn new(config: LeakConfig) -> Self {
        Self {
            snapshots: Arc::new(RwLock::new(Vec::new())),
            allocations: Arc::new(RwLock::new(HashMap::new())),
            leak_config: config,
            last_check: Arc::new(RwLock::new(Instant::now())),
        }
    }

    /// Takes a memory snapshot
    pub async fn take_snapshot(&self) -> Result<MemorySnapshot> {
        let mut total_heap = 0;
        let mut components = HashMap::new();
        let mut alloc_count = 0;

        let allocs = self.allocations.read().await;
        for (component, sizes) in allocs.iter() {
            let component_total: usize = sizes.iter().sum();
            total_heap += component_total;
            alloc_count += sizes.len();
            components.insert(component.clone(), component_total);
        }

        let snapshot = MemorySnapshot {
            heap_size: total_heap,
            component_usage: components,
            allocation_count: alloc_count,
            timestamp: Instant::now(),
        };

        let mut snapshots = self.snapshots.write().await;
        snapshots.push(snapshot.clone());

        // Keep last 100 snapshots
        while snapshots.len() > 100 {
            snapshots.remove(0);
        }

        Ok(snapshot)
    }

    /// Records a new allocation
    pub async fn record_allocation(&self, component: &str, size: usize) {
        let mut allocs = self.allocations.write().await;
        allocs.entry(component.to_string())
            .or_insert_with(Vec::new)
            .push(size);
    }

    /// Records memory being freed
    pub async fn record_free(&self, component: &str, size: usize) {
        let mut allocs = self.allocations.write().await;
        if let Some(sizes) = allocs.get_mut(component) {
            if let Some(pos) = sizes.iter().position(|&s| s == size) {
                sizes.swap_remove(pos);
            }
        }
    }

    /// Checks for potential memory leaks
    pub async fn check_leaks(&self) -> Vec<String> {
        let mut leaks = Vec::new();
        let now = Instant::now();
        let mut last_check = self.last_check.write().await;

        // Only check at configured interval
        if now.duration_since(*last_check) < self.leak_config.check_interval {
            return leaks;
        }
        *last_check = now;

        let snapshots = self.snapshots.read().await;
        if snapshots.len() < 2 {
            return leaks;
        }

        let oldest = &snapshots[0];
        let newest = &snapshots[snapshots.len() - 1];

        // Check overall heap growth
        let growth = (newest.heap_size as f64 - oldest.heap_size as f64) / oldest.heap_size as f64;
        if growth > self.leak_config.growth_threshold {
            leaks.push(format!("Total heap grew by {:.1}%", growth * 100.0));
        }

        // Check monitored components
        for component in &self.leak_config.monitored_components {
            let old_size = oldest.component_usage.get(component).copied().unwrap_or(0);
            let new_size = newest.component_usage.get(component).copied().unwrap_or(0);
            
            if old_size > 0 {
                let growth = (new_size as f64 - old_size as f64) / old_size as f64;
                if growth > self.leak_config.growth_threshold {
                    leaks.push(format!(
                        "Component {} grew by {:.1}%", 
                        component,
                        growth * 100.0
                    ));
                }
            }
        }

        leaks
    }
} 