use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{info, error};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    pub id: Uuid,
    pub service_name: String,
    pub version: String,
    pub host: String,
    pub port: u16,
    pub status: ServiceStatus,
    pub metadata: HashMap<String, String>,
    pub last_heartbeat: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceStatus {
    Starting,
    Running,
    Degraded,
    Stopping,
    Stopped,
}

#[derive(Debug, Clone)]
pub struct ServiceRegistry {
    services: RwLock<HashMap<String, Vec<ServiceInstance>>>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register(&self, instance: ServiceInstance) -> anyhow::Result<()> {
        let mut services = self.services.write().await;
        let instances = services
            .entry(instance.service_name.clone())
            .or_insert_with(Vec::new);
        
        // Check for duplicate instance
        if instances.iter().any(|i| i.id == instance.id) {
            return Err(anyhow::anyhow!("Service instance already registered"));
        }
        
        instances.push(instance.clone());
        info!("Registered service instance: {:?}", instance);
        Ok(())
    }

    pub async fn deregister(&self, service_name: &str, instance_id: Uuid) -> anyhow::Result<()> {
        let mut services = self.services.write().await;
        if let Some(instances) = services.get_mut(service_name) {
            instances.retain(|i| i.id != instance_id);
            info!("Deregistered service instance: {} - {}", service_name, instance_id);
        }
        Ok(())
    }

    pub async fn get_instances(&self, service_name: &str) -> Vec<ServiceInstance> {
        let services = self.services.read().await;
        services.get(service_name).cloned().unwrap_or_default()
    }

    pub async fn update_heartbeat(&self, service_name: &str, instance_id: Uuid) -> anyhow::Result<()> {
        let mut services = self.services.write().await;
        if let Some(instances) = services.get_mut(service_name) {
            if let Some(instance) = instances.iter_mut().find(|i| i.id == instance_id) {
                instance.last_heartbeat = Utc::now();
                info!("Updated heartbeat for service instance: {} - {}", service_name, instance_id);
            }
        }
        Ok(())
    }

    pub async fn get_healthy_instances(&self, service_name: &str) -> Vec<ServiceInstance> {
        let services = self.services.read().await;
        if let Some(instances) = services.get(service_name) {
            instances
                .iter()
                .filter(|i| i.status == ServiceStatus::Running)
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[tokio::test]
    async fn test_service_registry() {
        let registry = ServiceRegistry::new();
        
        let instance = ServiceInstance {
            id: Uuid::new_v4(),
            service_name: "test-service".to_string(),
            version: "1.0.0".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            status: ServiceStatus::Running,
            metadata: HashMap::new(),
            last_heartbeat: Utc::now(),
        };
        
        // Test registration
        registry.register(instance.clone()).await.unwrap();
        
        // Test getting instances
        let instances = registry.get_instances("test-service").await;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, instance.id);
        
        // Test heartbeat update
        registry.update_heartbeat("test-service", instance.id).await.unwrap();
        
        // Test getting healthy instances
        let healthy_instances = registry.get_healthy_instances("test-service").await;
        assert_eq!(healthy_instances.len(), 1);
        
        // Test deregistration
        registry.deregister("test-service", instance.id).await.unwrap();
        let instances = registry.get_instances("test-service").await;
        assert_eq!(instances.len(), 0);
    }
} 