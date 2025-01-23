//! Load balancing for connection management

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use crate::error::{Error, Result};
use super::{Connection, ConnectionMetrics, ConnectionStatus};

/// Load balancing strategies
#[derive(Debug, Clone, Copy)]
pub enum BalanceStrategy {
    /// Round robin distribution
    RoundRobin,
    /// Least connections
    LeastConnections,
    /// Least load (based on message counts)
    LeastLoad,
}

/// Load balancer configuration
#[derive(Debug, Clone)]
pub struct BalancerConfig {
    /// Load balancing strategy
    pub strategy: BalanceStrategy,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Maximum connections per node
    pub max_connections_per_node: u32,
}

impl Default for BalancerConfig {
    fn default() -> Self {
        Self {
            strategy: BalanceStrategy::LeastLoad,
            health_check_interval: Duration::from_secs(30),
            max_connections_per_node: 1000,
        }
    }
}

/// Node health status
#[derive(Debug, Clone)]
struct NodeHealth {
    last_check: Instant,
    is_healthy: bool,
    error_count: u32,
}

/// Load balancer for managing connection distribution
pub struct LoadBalancer {
    config: BalancerConfig,
    nodes: Arc<RwLock<HashMap<String, NodeHealth>>>,
    current_index: Arc<RwLock<usize>>,
}

impl LoadBalancer {
    pub fn new(config: BalancerConfig) -> Self {
        Self {
            config,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            current_index: Arc::new(RwLock::new(0)),
        }
    }

    /// Adds a node to the load balancer
    pub async fn add_node(&self, node_id: String) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        if nodes.len() >= self.config.max_connections_per_node as usize {
            return Err(Error::Connection("Maximum nodes reached".into()));
        }

        nodes.insert(node_id, NodeHealth {
            last_check: Instant::now(),
            is_healthy: true,
            error_count: 0,
        });
        Ok(())
    }

    /// Removes a node from the load balancer
    pub async fn remove_node(&self, node_id: &str) {
        let mut nodes = self.nodes.write().await;
        nodes.remove(node_id);
    }

    /// Gets the next node based on the balancing strategy
    pub async fn next_node(&self, connections: &HashMap<String, Connection>) -> Result<String> {
        let nodes = self.nodes.read().await;
        if nodes.is_empty() {
            return Err(Error::Connection("No available nodes".into()));
        }

        match self.config.strategy {
            BalanceStrategy::RoundRobin => self.round_robin(&nodes).await,
            BalanceStrategy::LeastConnections => self.least_connections(&nodes, connections).await,
            BalanceStrategy::LeastLoad => self.least_load(&nodes, connections).await,
        }
    }

    async fn round_robin(&self, nodes: &HashMap<String, NodeHealth>) -> Result<String> {
        let mut index = self.current_index.write().await;
        let healthy_nodes: Vec<_> = nodes.iter()
            .filter(|(_, health)| health.is_healthy)
            .collect();

        if healthy_nodes.is_empty() {
            return Err(Error::Connection("No healthy nodes available".into()));
        }

        *index = (*index + 1) % healthy_nodes.len();
        Ok(healthy_nodes[*index].0.clone())
    }

    async fn least_connections(
        &self,
        nodes: &HashMap<String, NodeHealth>,
        connections: &HashMap<String, Connection>,
    ) -> Result<String> {
        nodes.iter()
            .filter(|(_, health)| health.is_healthy)
            .min_by_key(|(node_id, _)| connections.iter()
                .filter(|(_, conn)| conn.id() == node_id)
                .count())
            .map(|(node_id, _)| node_id.clone())
            .ok_or_else(|| Error::Connection("No healthy nodes available".into()))
    }

    async fn least_load(
        &self,
        nodes: &HashMap<String, NodeHealth>,
        connections: &HashMap<String, Connection>,
    ) -> Result<String> {
        nodes.iter()
            .filter(|(_, health)| health.is_healthy)
            .min_by_key(|(node_id, _)| {
                connections.iter()
                    .filter(|(_, conn)| conn.id() == node_id)
                    .map(|(_, conn)| conn.metrics().get_metrics().messages_sent)
                    .sum::<u64>()
            })
            .map(|(node_id, _)| node_id.clone())
            .ok_or_else(|| Error::Connection("No healthy nodes available".into()))
    }

    /// Updates node health status
    pub async fn update_health(&self, node_id: &str, is_healthy: bool) {
        let mut nodes = self.nodes.write().await;
        if let Some(health) = nodes.get_mut(node_id) {
            health.last_check = Instant::now();
            health.is_healthy = is_healthy;
            if !is_healthy {
                health.error_count += 1;
            } else {
                health.error_count = 0;
            }
        }
    }
} 