//! Mobile app location services and geofencing
//! Handles location tracking, geofencing, and location-based features

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Location accuracy level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocationAccuracy {
    /// High accuracy (GPS)
    High,
    /// Medium accuracy (Network + GPS)
    Medium,
    /// Low accuracy (Network only)
    Low,
    /// Best available accuracy
    Best,
}

/// Location permission status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocationPermission {
    /// Permission granted
    Granted,
    /// Permission denied
    Denied,
    /// Permission restricted
    Restricted,
    /// Permission not determined
    NotDetermined,
}

/// Location update frequency
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UpdateFrequency {
    /// Continuous updates
    Continuous,
    /// Updates every few seconds
    Frequent,
    /// Updates every few minutes
    Periodic,
    /// Updates only when significant movement
    Significant,
    /// Single update
    Single,
}

/// Location data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationData {
    /// Latitude
    pub latitude: f64,
    /// Longitude
    pub longitude: f64,
    /// Altitude in meters
    pub altitude: Option<f64>,
    /// Accuracy in meters
    pub accuracy: f64,
    /// Speed in meters per second
    pub speed: Option<f64>,
    /// Bearing in degrees
    pub bearing: Option<f64>,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Geofence region
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeofenceRegion {
    /// Region ID
    pub id: String,
    /// Region name
    pub name: String,
    /// Center latitude
    pub latitude: f64,
    /// Center longitude
    pub longitude: f64,
    /// Radius in meters
    pub radius: f64,
    /// Whether region is active
    pub active: bool,
    /// Whether to notify on entry
    pub notify_on_entry: bool,
    /// Whether to notify on exit
    pub notify_on_exit: bool,
    /// Whether to notify on dwell
    pub notify_on_dwell: bool,
    /// Dwell time in seconds
    pub dwell_time: Option<Duration>,
}

/// Location configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationConfig {
    /// Required accuracy level
    pub accuracy: LocationAccuracy,
    /// Update frequency
    pub update_frequency: UpdateFrequency,
    /// Minimum distance for updates in meters
    pub min_distance: f64,
    /// Maximum age of location data in seconds
    pub max_age: Duration,
    /// Whether to request background updates
    pub background_updates: bool,
    /// Whether to pause updates when app is inactive
    pub pause_updates: bool,
    /// Whether to use significant location changes
    pub significant_changes: bool,
    /// Whether to use geofencing
    pub use_geofencing: bool,
    /// Maximum number of geofences
    pub max_geofences: usize,
}

impl Default for LocationConfig {
    fn default() -> Self {
        Self {
            accuracy: LocationAccuracy::Medium,
            update_frequency: UpdateFrequency::Periodic,
            min_distance: 10.0,
            max_age: Duration::from_secs(300), // 5 minutes
            background_updates: false,
            pause_updates: true,
            significant_changes: false,
            use_geofencing: true,
            max_geofences: 20,
        }
    }
}

/// Location manager
pub struct LocationManager {
    /// Location configuration
    config: LocationConfig,
    /// Current location
    current_location: Arc<RwLock<Option<LocationData>>>,
    /// Location permission status
    permission_status: Arc<RwLock<LocationPermission>>,
    /// Active geofences
    geofences: Arc<RwLock<HashMap<String, GeofenceRegion>>>,
    /// Platform-specific location handlers
    handlers: Arc<RwLock<HashMap<String, Box<dyn LocationHandler>>>>,
    /// Location update channel
    update_tx: tokio::sync::mpsc::Sender<LocationData>,
    /// Location update receiver
    update_rx: tokio::sync::mpsc::Receiver<LocationData>,
}

/// Location handler trait
#[async_trait::async_trait]
pub trait LocationHandler: Send + Sync {
    /// Initialize the location handler
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Request location permission
    async fn request_permission(&self) -> Result<LocationPermission, Box<dyn std::error::Error>>;
    
    /// Check location permission
    async fn check_permission(&self) -> Result<LocationPermission, Box<dyn std::error::Error>>;
    
    /// Start location updates
    async fn start_updates(&self, config: &LocationConfig) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Stop location updates
    async fn stop_updates(&self) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Get current location
    async fn get_current_location(&self) -> Result<LocationData, Box<dyn std::error::Error>>;
    
    /// Add geofence region
    async fn add_geofence(&self, region: &GeofenceRegion) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Remove geofence region
    async fn remove_geofence(&self, region_id: &str) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Get all geofence regions
    async fn get_geofences(&self) -> Result<Vec<GeofenceRegion>, Box<dyn std::error::Error>>;
}

impl LocationManager {
    /// Create a new location manager
    pub fn new(config: LocationConfig) -> Self {
        let (update_tx, update_rx) = tokio::sync::mpsc::channel(100);
        
        Self {
            config,
            current_location: Arc::new(RwLock::new(None)),
            permission_status: Arc::new(RwLock::new(LocationPermission::NotDetermined)),
            geofences: Arc::new(RwLock::new(HashMap::new())),
            handlers: Arc::new(RwLock::new(HashMap::new())),
            update_tx,
            update_rx,
        }
    }
    
    /// Register a location handler
    pub async fn register_handler<H>(&self, platform: &str, handler: H)
    where
        H: LocationHandler + 'static,
    {
        let mut handlers = self.handlers.write().await;
        handlers.insert(platform.to_string(), Box::new(handler));
    }
    
    /// Request location permission
    pub async fn request_permission(&self) -> Result<LocationPermission, Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        let mut permission = LocationPermission::NotDetermined;
        
        for handler in handlers.values() {
            match handler.request_permission().await {
                Ok(status) => {
                    permission = status;
                    break;
                }
                Err(e) => {
                    warn!("Error requesting permission: {}", e);
                }
            }
        }
        
        let mut status = self.permission_status.write().await;
        *status = permission;
        
        Ok(permission)
    }
    
    /// Start location updates
    pub async fn start_updates(&self) -> Result<(), Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        
        for handler in handlers.values() {
            if let Err(e) = handler.start_updates(&self.config).await {
                warn!("Error starting location updates: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// Stop location updates
    pub async fn stop_updates(&self) -> Result<(), Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        
        for handler in handlers.values() {
            if let Err(e) = handler.stop_updates().await {
                warn!("Error stopping location updates: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// Get current location
    pub async fn get_current_location(&self) -> Result<LocationData, Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        
        for handler in handlers.values() {
            match handler.get_current_location().await {
                Ok(location) => {
                    let mut current = self.current_location.write().await;
                    *current = Some(location.clone());
                    return Ok(location);
                }
                Err(e) => {
                    warn!("Error getting current location: {}", e);
                }
            }
        }
        
        Err("Failed to get current location".into())
    }
    
    /// Add geofence region
    pub async fn add_geofence(&self, region: GeofenceRegion) -> Result<(), Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        let mut geofences = self.geofences.write().await;
        
        // Check maximum geofences
        if geofences.len() >= self.config.max_geofences {
            return Err("Maximum number of geofences reached".into());
        }
        
        // Add to handlers
        for handler in handlers.values() {
            if let Err(e) = handler.add_geofence(&region).await {
                warn!("Error adding geofence: {}", e);
            }
        }
        
        // Add to local storage
        geofences.insert(region.id.clone(), region);
        
        Ok(())
    }
    
    /// Remove geofence region
    pub async fn remove_geofence(&self, region_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        let mut geofences = self.geofences.write().await;
        
        // Remove from handlers
        for handler in handlers.values() {
            if let Err(e) = handler.remove_geofence(region_id).await {
                warn!("Error removing geofence: {}", e);
            }
        }
        
        // Remove from local storage
        geofences.remove(region_id);
        
        Ok(())
    }
    
    /// Get all geofence regions
    pub async fn get_geofences(&self) -> Result<Vec<GeofenceRegion>, Box<dyn std::error::Error>> {
        let handlers = self.handlers.read().await;
        let geofences = self.geofences.read().await;
        
        // Get from handlers
        for handler in handlers.values() {
            match handler.get_geofences().await {
                Ok(regions) => {
                    return Ok(regions);
                }
                Err(e) => {
                    warn!("Error getting geofences: {}", e);
                }
            }
        }
        
        // Return from local storage
        Ok(geofences.values().cloned().collect())
    }
    
    /// Process location updates
    pub async fn process_updates(&mut self) {
        while let Some(location) = self.update_rx.recv().await {
            let mut current = self.current_location.write().await;
            *current = Some(location.clone());
            
            // Check geofences
            if self.config.use_geofencing {
                let geofences = self.geofences.read().await;
                for region in geofences.values() {
                    if region.active {
                        let distance = self.calculate_distance(
                            location.latitude,
                            location.longitude,
                            region.latitude,
                            region.longitude,
                        );
                        
                        if distance <= region.radius {
                            info!("Entered geofence region: {}", region.name);
                        } else {
                            info!("Exited geofence region: {}", region.name);
                        }
                    }
                }
            }
        }
    }
    
    /// Calculate distance between two points using Haversine formula
    fn calculate_distance(&self, lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
        const R: f64 = 6371000.0; // Earth's radius in meters
        
        let phi1 = lat1.to_radians();
        let phi2 = lat2.to_radians();
        let delta_phi = (lat2 - lat1).to_radians();
        let delta_lambda = (lon2 - lon1).to_radians();
        
        let a = (delta_phi / 2.0).sin().powi(2) +
            phi1.cos() * phi2.cos() * (delta_lambda / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
        
        R * c
    }
}

/// Example location handler implementation
pub struct ConsoleLocationHandler;

#[async_trait::async_trait]
impl LocationHandler for ConsoleLocationHandler {
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing console location handler");
        Ok(())
    }
    
    async fn request_permission(&self) -> Result<LocationPermission, Box<dyn std::error::Error>> {
        info!("Requesting console location permission");
        Ok(LocationPermission::Granted)
    }
    
    async fn check_permission(&self) -> Result<LocationPermission, Box<dyn std::error::Error>> {
        info!("Checking console location permission");
        Ok(LocationPermission::Granted)
    }
    
    async fn start_updates(&self, config: &LocationConfig) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting console location updates: {:?}", config);
        Ok(())
    }
    
    async fn stop_updates(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Stopping console location updates");
        Ok(())
    }
    
    async fn get_current_location(&self) -> Result<LocationData, Box<dyn std::error::Error>> {
        info!("Getting console current location");
        Ok(LocationData {
            latitude: 37.7749,
            longitude: -122.4194,
            altitude: Some(0.0),
            accuracy: 10.0,
            speed: Some(0.0),
            bearing: Some(0.0),
            timestamp: chrono::Utc::now(),
        })
    }
    
    async fn add_geofence(&self, region: &GeofenceRegion) -> Result<(), Box<dyn std::error::Error>> {
        info!("Adding console geofence: {:?}", region);
        Ok(())
    }
    
    async fn remove_geofence(&self, region_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("Removing console geofence: {}", region_id);
        Ok(())
    }
    
    async fn get_geofences(&self) -> Result<Vec<GeofenceRegion>, Box<dyn std::error::Error>> {
        info!("Getting console geofences");
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_location_operations() {
        let config = LocationConfig::default();
        let mut manager = LocationManager::new(config);
        
        // Register console handler
        manager.register_handler("console", ConsoleLocationHandler).await;
        
        // Request permission
        let permission = manager.request_permission().await.unwrap();
        assert_eq!(permission, LocationPermission::Granted);
        
        // Get current location
        let location = manager.get_current_location().await.unwrap();
        assert_eq!(location.latitude, 37.7749);
        assert_eq!(location.longitude, -122.4194);
    }
    
    #[tokio::test]
    async fn test_geofence_operations() {
        let config = LocationConfig::default();
        let manager = LocationManager::new(config);
        
        // Register console handler
        manager.register_handler("console", ConsoleLocationHandler).await;
        
        // Add geofence
        let region = GeofenceRegion {
            id: "test".to_string(),
            name: "Test Region".to_string(),
            latitude: 37.7749,
            longitude: -122.4194,
            radius: 100.0,
            active: true,
            notify_on_entry: true,
            notify_on_exit: true,
            notify_on_dwell: false,
            dwell_time: None,
        };
        
        manager.add_geofence(region.clone()).await.unwrap();
        
        // Get geofences
        let regions = manager.get_geofences().await.unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].name, "Test Region");
    }
} 