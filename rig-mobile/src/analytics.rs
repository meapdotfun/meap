//! Mobile app analytics and telemetry
//! Handles tracking of app usage, performance metrics, and user behavior

use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Analytics event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    /// App lifecycle events
    AppStart,
    AppBackground,
    AppForeground,
    AppCrash,
    
    /// User interaction events
    ScreenView,
    ButtonClick,
    Gesture,
    Search,
    
    /// Performance events
    NetworkRequest,
    DatabaseOperation,
    CacheHit,
    CacheMiss,
    
    /// Feature usage events
    FeatureEnable,
    FeatureDisable,
    FeatureError,
    
    /// Custom event type
    Custom(String),
}

/// Analytics event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsEvent {
    /// Event type
    pub event_type: EventType,
    /// Event timestamp
    pub timestamp: i64,
    /// Event duration (if applicable)
    pub duration: Option<Duration>,
    /// Event properties
    pub properties: HashMap<String, serde_json::Value>,
    /// Device info
    pub device_info: crate::device::DeviceInfo,
    /// Session ID
    pub session_id: String,
}

/// Analytics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsConfig {
    /// Maximum number of events to store in memory
    pub max_events: usize,
    /// Maximum age of events to keep
    pub max_event_age: Duration,
    /// Whether to enable crash reporting
    pub enable_crash_reporting: bool,
    /// Whether to enable performance monitoring
    pub enable_performance_monitoring: bool,
    /// Whether to enable user behavior tracking
    pub enable_user_tracking: bool,
    /// Custom event filters
    pub event_filters: Vec<String>,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            max_events: 1000,
            max_event_age: Duration::from_secs(24 * 60 * 60), // 24 hours
            enable_crash_reporting: true,
            enable_performance_monitoring: true,
            enable_user_tracking: true,
            event_filters: Vec::new(),
        }
    }
}

/// Analytics manager
pub struct AnalyticsManager {
    /// Analytics configuration
    config: AnalyticsConfig,
    /// Event storage
    events: Arc<RwLock<VecDeque<AnalyticsEvent>>>,
    /// Performance metrics
    performance_metrics: Arc<RwLock<HashMap<String, Vec<Duration>>>>,
    /// User session tracking
    sessions: Arc<RwLock<HashMap<String, Instant>>>,
    /// Event processors
    processors: Arc<RwLock<Vec<Box<dyn AnalyticsProcessor>>>>,
}

/// Analytics processor trait
#[async_trait::async_trait]
pub trait AnalyticsProcessor: Send + Sync {
    /// Process an analytics event
    async fn process_event(&self, event: &AnalyticsEvent) -> Result<(), Box<dyn std::error::Error>>;
}

impl AnalyticsManager {
    /// Create a new analytics manager
    pub fn new(config: AnalyticsConfig) -> Self {
        Self {
            config,
            events: Arc::new(RwLock::new(VecDeque::new())),
            performance_metrics: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            processors: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Register an analytics processor
    pub async fn register_processor<P>(&self, processor: P)
    where
        P: AnalyticsProcessor + 'static,
    {
        let mut processors = self.processors.write().await;
        processors.push(Box::new(processor));
    }
    
    /// Track an analytics event
    pub async fn track_event(&self, event: AnalyticsEvent) -> Result<(), Box<dyn std::error::Error>> {
        // Check if event should be filtered
        if let EventType::Custom(event_name) = &event.event_type {
            if self.config.event_filters.contains(event_name) {
                return Ok(());
            }
        }
        
        // Store event
        {
            let mut events = self.events.write().await;
            events.push_back(event.clone());
            
            // Trim old events
            while events.len() > self.config.max_events {
                if let Some(oldest) = events.pop_front() {
                    if let Some(duration) = oldest.duration {
                        if duration > self.config.max_event_age {
                            continue;
                        }
                    }
                }
            }
        }
        
        // Process event through all registered processors
        let processors = self.processors.read().await;
        for processor in processors.iter() {
            if let Err(e) = processor.process_event(&event).await {
                warn!("Error processing analytics event: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// Track a performance metric
    pub async fn track_performance(&self, metric_name: &str, duration: Duration) {
        if !self.config.enable_performance_monitoring {
            return;
        }
        
        let mut metrics = self.performance_metrics.write().await;
        let metric_values = metrics.entry(metric_name.to_string())
            .or_insert_with(Vec::new);
        
        metric_values.push(duration);
        
        // Keep only recent metrics
        let cutoff = Instant::now() - self.config.max_event_age;
        metric_values.retain(|&d| d < self.config.max_event_age);
    }
    
    /// Start a new user session
    pub async fn start_session(&self, session_id: String) {
        if !self.config.enable_user_tracking {
            return;
        }
        
        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id, Instant::now());
    }
    
    /// End a user session
    pub async fn end_session(&self, session_id: &str) {
        if !self.config.enable_user_tracking {
            return;
        }
        
        let mut sessions = self.sessions.write().await;
        if let Some(start_time) = sessions.remove(session_id) {
            let duration = start_time.elapsed();
            
            // Track session duration
            self.track_event(AnalyticsEvent {
                event_type: EventType::Custom("session_end".to_string()),
                timestamp: chrono::Utc::now().timestamp(),
                duration: Some(duration),
                properties: HashMap::new(),
                device_info: crate::device::DeviceInfo::default(), // TODO: Get actual device info
                session_id: session_id.to_string(),
            }).await.ok();
        }
    }
    
    /// Get analytics summary
    pub async fn get_summary(&self) -> AnalyticsSummary {
        let events = self.events.read().await;
        let metrics = self.performance_metrics.read().await;
        let sessions = self.sessions.read().await;
        
        AnalyticsSummary {
            total_events: events.len(),
            active_sessions: sessions.len(),
            performance_metrics: metrics.clone(),
            event_counts: events.iter()
                .fold(HashMap::new(), |mut acc, event| {
                    let event_type = match &event.event_type {
                        EventType::Custom(name) => name.clone(),
                        _ => format!("{:?}", event.event_type),
                    };
                    *acc.entry(event_type).or_insert(0) += 1;
                    acc
                }),
        }
    }
}

/// Analytics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsSummary {
    /// Total number of events
    pub total_events: usize,
    /// Number of active sessions
    pub active_sessions: usize,
    /// Performance metrics
    pub performance_metrics: HashMap<String, Vec<Duration>>,
    /// Event type counts
    pub event_counts: HashMap<String, usize>,
}

/// Example analytics processor implementation
pub struct ConsoleAnalyticsProcessor;

#[async_trait::async_trait]
impl AnalyticsProcessor for ConsoleAnalyticsProcessor {
    async fn process_event(&self, event: &AnalyticsEvent) -> Result<(), Box<dyn std::error::Error>> {
        info!("Analytics event: {:?}", event);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_analytics_tracking() {
        let config = AnalyticsConfig::default();
        let manager = AnalyticsManager::new(config);
        
        // Register console processor
        manager.register_processor(ConsoleAnalyticsProcessor).await;
        
        // Track some events
        let event = AnalyticsEvent {
            event_type: EventType::AppStart,
            timestamp: chrono::Utc::now().timestamp(),
            duration: None,
            properties: HashMap::new(),
            device_info: crate::device::DeviceInfo::default(),
            session_id: "test-session".to_string(),
        };
        
        manager.track_event(event).await.unwrap();
        
        // Track performance
        manager.track_performance("app_startup", Duration::from_millis(100)).await;
        
        // Start and end session
        manager.start_session("test-session".to_string()).await;
        manager.end_session("test-session").await;
        
        // Get summary
        let summary = manager.get_summary().await;
        
        assert_eq!(summary.total_events, 2); // AppStart + session_end
        assert_eq!(summary.active_sessions, 0);
        assert!(summary.performance_metrics.contains_key("app_startup"));
        assert!(summary.event_counts.contains_key("AppStart"));
    }
    
    #[tokio::test]
    async fn test_event_filtering() {
        let mut config = AnalyticsConfig::default();
        config.event_filters.push("filtered_event".to_string());
        
        let manager = AnalyticsManager::new(config);
        
        // Track filtered event
        let event = AnalyticsEvent {
            event_type: EventType::Custom("filtered_event".to_string()),
            timestamp: chrono::Utc::now().timestamp(),
            duration: None,
            properties: HashMap::new(),
            device_info: crate::device::DeviceInfo::default(),
            session_id: "test-session".to_string(),
        };
        
        manager.track_event(event).await.unwrap();
        
        // Get summary
        let summary = manager.get_summary().await;
        
        assert_eq!(summary.total_events, 0);
    }
} 