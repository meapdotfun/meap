//! Mobile device detection and capabilities
//! Handles platform-specific features for iOS and Android devices

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Mobile platform types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    /// iOS devices (iPhone, iPad, iPod)
    iOS,
    /// Android devices
    Android,
    /// Web browser
    Web,
    /// Unknown platform
    Unknown,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::iOS => write!(f, "iOS"),
            Platform::Android => write!(f, "Android"),
            Platform::Web => write!(f, "Web"),
            Platform::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Device form factors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FormFactor {
    /// Phone-sized device
    Phone,
    /// Tablet-sized device
    Tablet,
    /// Foldable device
    Foldable,
    /// Desktop browser
    Desktop,
    /// Unknown form factor
    Unknown,
}

/// Device capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    /// Device has camera
    pub camera: bool,
    /// Device has biometric authentication
    pub biometric: bool,
    /// Device has NFC
    pub nfc: bool,
    /// Device has GPS
    pub gps: bool,
    /// Device has accelerometer
    pub accelerometer: bool,
    /// Device has gyroscope
    pub gyroscope: bool,
    /// Device has push notification support
    pub push_notifications: bool,
    /// Device has Bluetooth
    pub bluetooth: bool,
    /// Device has AR support
    pub ar_support: bool,
    /// Device has dark mode support
    pub dark_mode: bool,
    /// Custom capabilities
    pub custom: HashMap<String, bool>,
}

impl Default for DeviceCapabilities {
    fn default() -> Self {
        Self {
            camera: false,
            biometric: false,
            nfc: false,
            gps: false,
            accelerometer: false,
            gyroscope: false,
            push_notifications: false,
            bluetooth: false,
            ar_support: false,
            dark_mode: false,
            custom: HashMap::new(),
        }
    }
}

/// Device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Device platform
    pub platform: Platform,
    /// Device form factor
    pub form_factor: FormFactor,
    /// Device model
    pub model: String,
    /// OS version
    pub os_version: String,
    /// App version
    pub app_version: String,
    /// Screen width in pixels
    pub screen_width: u32,
    /// Screen height in pixels
    pub screen_height: u32,
    /// Device pixel ratio
    pub pixel_ratio: f32,
    /// Device language
    pub language: String,
    /// Device timezone
    pub timezone: String,
    /// Device capabilities
    pub capabilities: DeviceCapabilities,
    /// Is this a simulator/emulator
    pub is_emulator: bool,
    /// Device unique identifier (if available)
    pub device_id: Option<String>,
}

/// User agent parser for device detection
pub struct UserAgentParser {
    /// iOS device patterns
    ios_patterns: Vec<(regex::Regex, &'static str)>,
    /// Android device patterns
    android_patterns: Vec<(regex::Regex, &'static str)>,
    /// Cache of parsed user agents
    cache: Arc<RwLock<HashMap<String, DeviceInfo>>>,
}

impl UserAgentParser {
    /// Create a new user agent parser
    pub fn new() -> Self {
        let ios_patterns = vec![
            (regex::Regex::new(r"iPhone(?:/(\d+\.\d+))?").unwrap(), "iPhone"),
            (regex::Regex::new(r"iPad(?:/(\d+\.\d+))?").unwrap(), "iPad"),
            (regex::Regex::new(r"iPod(?:/(\d+\.\d+))?").unwrap(), "iPod"),
        ];
        
        let android_patterns = vec![
            (regex::Regex::new(r"Android (\d+\.\d+)").unwrap(), "Android"),
            (regex::Regex::new(r"SM-[A-Z0-9]+").unwrap(), "Samsung"),
            (regex::Regex::new(r"Pixel (\d+)").unwrap(), "Google Pixel"),
            (regex::Regex::new(r"OnePlus").unwrap(), "OnePlus"),
        ];
        
        Self {
            ios_patterns,
            android_patterns,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Parse a user agent string
    pub async fn parse(&self, user_agent: &str) -> DeviceInfo {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(info) = cache.get(user_agent) {
                return info.clone();
            }
        }
        
        let mut info = DeviceInfo {
            platform: Platform::Unknown,
            form_factor: FormFactor::Unknown,
            model: "Unknown".to_string(),
            os_version: "Unknown".to_string(),
            app_version: "Unknown".to_string(),
            screen_width: 0,
            screen_height: 0,
            pixel_ratio: 1.0,
            language: "en".to_string(),
            timezone: "UTC".to_string(),
            capabilities: DeviceCapabilities::default(),
            is_emulator: false,
            device_id: None,
        };
        
        // Detect iOS devices
        for (pattern, model_name) in &self.ios_patterns {
            if let Some(captures) = pattern.captures(user_agent) {
                info.platform = Platform::iOS;
                info.model = model_name.to_string();
                
                if let Some(version) = captures.get(1) {
                    info.os_version = version.as_str().to_string();
                }
                
                // Set form factor
                if model_name == "iPad" {
                    info.form_factor = FormFactor::Tablet;
                } else {
                    info.form_factor = FormFactor::Phone;
                }
                
                // Set common iOS capabilities
                info.capabilities.camera = true;
                info.capabilities.biometric = true;
                info.capabilities.gps = true;
                info.capabilities.accelerometer = true;
                info.capabilities.gyroscope = true;
                info.capabilities.push_notifications = true;
                info.capabilities.bluetooth = true;
                info.capabilities.dark_mode = true;
                
                // Check for simulator
                if user_agent.contains("Simulator") {
                    info.is_emulator = true;
                }
                
                break;
            }
        }
        
        // Detect Android devices
        if info.platform == Platform::Unknown {
            for (pattern, model_name) in &self.android_patterns {
                if let Some(captures) = pattern.captures(user_agent) {
                    info.platform = Platform::Android;
                    
                    if model_name == "Android" {
                        if let Some(version) = captures.get(1) {
                            info.os_version = version.as_str().to_string();
                        }
                        
                        // Try to extract device model
                        if let Some(model) = extract_android_model(user_agent) {
                            info.model = model;
                        } else {
                            info.model = "Android Device".to_string();
                        }
                    } else {
                        info.model = model_name.to_string();
                        
                        // Try to extract version
                        if let Some(version) = captures.get(1) {
                            info.model = format!("{} {}", model_name, version.as_str());
                        }
                    }
                    
                    // Detect form factor (rough estimate based on user agent)
                    if user_agent.contains("tablet") || user_agent.contains("Tab") {
                        info.form_factor = FormFactor::Tablet;
                    } else if user_agent.contains("fold") || user_agent.contains("Fold") {
                        info.form_factor = FormFactor::Foldable;
                    } else {
                        info.form_factor = FormFactor::Phone;
                    }
                    
                    // Set common Android capabilities
                    info.capabilities.camera = true;
                    info.capabilities.gps = true;
                    info.capabilities.accelerometer = true;
                    info.capabilities.push_notifications = true;
                    info.capabilities.bluetooth = true;
                    info.capabilities.dark_mode = true;
                    
                    // Check for emulator
                    if user_agent.contains("sdk_gphone") || user_agent.contains("Android SDK") {
                        info.is_emulator = true;
                    }
                    
                    break;
                }
            }
        }
        
        // Detect web browser
        if info.platform == Platform::Unknown && (
            user_agent.contains("Mozilla") || 
            user_agent.contains("Chrome") || 
            user_agent.contains("Safari") ||
            user_agent.contains("Firefox") ||
            user_agent.contains("Edge")
        ) {
            info.platform = Platform::Web;
            info.form_factor = FormFactor::Desktop;
            
            // Extract browser info
            if user_agent.contains("Chrome/") {
                info.model = "Chrome".to_string();
            } else if user_agent.contains("Safari/") {
                info.model = "Safari".to_string();
            } else if user_agent.contains("Firefox/") {
                info.model = "Firefox".to_string();
            } else if user_agent.contains("Edge/") {
                info.model = "Edge".to_string();
            } else {
                info.model = "Web Browser".to_string();
            }
            
            // Set web capabilities
            info.capabilities.camera = user_agent.contains("Chrome") || user_agent.contains("Safari");
            info.capabilities.push_notifications = true;
            info.capabilities.dark_mode = true;
        }
        
        // Cache the result
        {
            let mut cache = self.cache.write().await;
            cache.insert(user_agent.to_string(), info.clone());
        }
        
        info
    }
    
    /// Clear the parser cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

/// Extract Android model from user agent
fn extract_android_model(user_agent: &str) -> Option<String> {
    // Common Android model patterns
    let model_patterns = [
        regex::Regex::new(r"SM-[A-Z0-9]+").ok()?, // Samsung
        regex::Regex::new(r"Pixel \d+").ok()?,    // Google Pixel
        regex::Regex::new(r"OnePlus\d+").ok()?,   // OnePlus
        regex::Regex::new(r"Redmi [A-Z0-9]+").ok()?, // Xiaomi Redmi
        regex::Regex::new(r"Mi [A-Z0-9]+").ok()?, // Xiaomi Mi
    ];
    
    for pattern in &model_patterns {
        if let Some(captures) = pattern.captures(user_agent) {
            if let Some(model) = captures.get(0) {
                return Some(model.as_str().to_string());
            }
        }
    }
    
    None
}

/// Device feature detection
pub struct DeviceFeatureDetector {
    /// Feature detection functions
    detectors: HashMap<String, Box<dyn Fn(&DeviceInfo) -> bool + Send + Sync>>,
}

impl DeviceFeatureDetector {
    /// Create a new feature detector
    pub fn new() -> Self {
        let mut detector = Self {
            detectors: HashMap::new(),
        };
        
        // Register default detectors
        detector.register_detector("biometric", Box::new(|info| {
            info.capabilities.biometric
        }));
        
        detector.register_detector("camera", Box::new(|info| {
            info.capabilities.camera
        }));
        
        detector.register_detector("nfc", Box::new(|info| {
            info.capabilities.nfc
        }));
        
        detector.register_detector("ar", Box::new(|info| {
            info.capabilities.ar_support
        }));
        
        detector.register_detector("dark_mode", Box::new(|info| {
            info.capabilities.dark_mode
        }));
        
        detector.register_detector("high_end_device", Box::new(|info| {
            // Simple heuristic for high-end devices
            match info.platform {
                Platform::iOS => {
                    // iPhone X or newer
                    info.model.contains("iPhone") && 
                    (info.model.contains("X") || 
                     info.model.contains("11") || 
                     info.model.contains("12") || 
                     info.model.contains("13") || 
                     info.model.contains("14") || 
                     info.model.contains("15"))
                },
                Platform::Android => {
                    // High-end Android devices
                    info.model.contains("Pixel") || 
                    info.model.contains("Galaxy S") || 
                    info.model.contains("OnePlus") || 
                    info.model.contains("Pro")
                },
                _ => false,
            }
        }));
        
        detector
    }
    
    /// Register a custom feature detector
    pub fn register_detector<F>(&mut self, name: &str, detector: Box<F>)
    where
        F: Fn(&DeviceInfo) -> bool + Send + Sync + 'static,
    {
        self.detectors.insert(name.to_string(), detector);
    }
    
    /// Check if a device supports a feature
    pub fn supports_feature(&self, info: &DeviceInfo, feature: &str) -> bool {
        if let Some(detector) = self.detectors.get(feature) {
            detector(info)
        } else {
            // Check custom capabilities
            info.capabilities.custom.get(feature).copied().unwrap_or(false)
        }
    }
    
    /// Get all supported features for a device
    pub fn get_supported_features(&self, info: &DeviceInfo) -> Vec<String> {
        self.detectors.iter()
            .filter_map(|(name, detector)| {
                if detector(info) {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Mobile app configuration based on device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Minimum supported iOS version
    pub min_ios_version: String,
    /// Minimum supported Android version
    pub min_android_version: String,
    /// Feature flags
    pub feature_flags: HashMap<String, bool>,
    /// Platform-specific settings
    pub platform_settings: HashMap<Platform, HashMap<String, serde_json::Value>>,
}

impl AppConfig {
    /// Check if a device is supported
    pub fn is_device_supported(&self, info: &DeviceInfo) -> bool {
        match info.platform {
            Platform::iOS => {
                version_compare(&info.os_version, &self.min_ios_version) >= 0
            },
            Platform::Android => {
                version_compare(&info.os_version, &self.min_android_version) >= 0
            },
            _ => true, // Web and unknown platforms are always supported
        }
    }
    
    /// Get settings for a specific platform
    pub fn get_platform_settings(&self, platform: Platform) -> HashMap<String, serde_json::Value> {
        self.platform_settings.get(&platform)
            .cloned()
            .unwrap_or_default()
    }
    
    /// Check if a feature flag is enabled
    pub fn is_feature_enabled(&self, feature: &str) -> bool {
        self.feature_flags.get(feature).copied().unwrap_or(false)
    }
    
    /// Check if a feature is enabled for a specific device
    pub fn is_feature_enabled_for_device(&self, feature: &str, info: &DeviceInfo, detector: &DeviceFeatureDetector) -> bool {
        // First check if the feature is globally enabled
        if !self.is_feature_enabled(feature) {
            return false;
        }
        
        // Then check if the device supports it
        detector.supports_feature(info, feature)
    }
}

/// Compare version strings
fn version_compare(version1: &str, version2: &str) -> i32 {
    let parts1: Vec<u32> = version1.split('.')
        .map(|s| s.parse().unwrap_or(0))
        .collect();
    
    let parts2: Vec<u32> = version2.split('.')
        .map(|s| s.parse().unwrap_or(0))
        .collect();
    
    let max_len = parts1.len().max(parts2.len());
    
    for i in 0..max_len {
        let v1 = parts1.get(i).copied().unwrap_or(0);
        let v2 = parts2.get(i).copied().unwrap_or(0);
        
        if v1 > v2 {
            return 1;
        } else if v1 < v2 {
            return -1;
        }
    }
    
    0 // Versions are equal
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_ios_detection() {
        let parser = UserAgentParser::new();
        
        let user_agent = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.0 Mobile/15E148 Safari/604.1";
        let info = parser.parse(user_agent).await;
        
        assert_eq!(info.platform, Platform::iOS);
        assert_eq!(info.form_factor, FormFactor::Phone);
        assert_eq!(info.model, "iPhone");
        assert!(info.capabilities.camera);
        assert!(info.capabilities.biometric);
    }
    
    #[tokio::test]
    async fn test_android_detection() {
        let parser = UserAgentParser::new();
        
        let user_agent = "Mozilla/5.0 (Linux; Android 11; SM-G998B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.120 Mobile Safari/537.36";
        let info = parser.parse(user_agent).await;
        
        assert_eq!(info.platform, Platform::Android);
        assert_eq!(info.form_factor, FormFactor::Phone);
        assert_eq!(info.os_version, "11");
        assert!(info.capabilities.camera);
        assert!(info.capabilities.gps);
    }
    
    #[tokio::test]
    async fn test_web_detection() {
        let parser = UserAgentParser::new();
        
        let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36";
        let info = parser.parse(user_agent).await;
        
        assert_eq!(info.platform, Platform::Web);
        assert_eq!(info.form_factor, FormFactor::Desktop);
        assert_eq!(info.model, "Chrome");
    }
    
    #[test]
    fn test_feature_detection() {
        let detector = DeviceFeatureDetector::new();
        
        let ios_info = DeviceInfo {
            platform: Platform::iOS,
            form_factor: FormFactor::Phone,
            model: "iPhone 13".to_string(),
            os_version: "15.0".to_string(),
            app_version: "1.0.0".to_string(),
            screen_width: 390,
            screen_height: 844,
            pixel_ratio: 3.0,
            language: "en".to_string(),
            timezone: "America/New_York".to_string(),
            capabilities: DeviceCapabilities {
                camera: true,
                biometric: true,
                nfc: true,
                gps: true,
                accelerometer: true,
                gyroscope: true,
                push_notifications: true,
                bluetooth: true,
                ar_support: true,
                dark_mode: true,
                custom: HashMap::new(),
            },
            is_emulator: false,
            device_id: None,
        };
        
        assert!(detector.supports_feature(&ios_info, "biometric"));
        assert!(detector.supports_feature(&ios_info, "camera"));
        assert!(detector.supports_feature(&ios_info, "high_end_device"));
        
        let features = detector.get_supported_features(&ios_info);
        assert!(features.contains(&"biometric".to_string()));
        assert!(features.contains(&"camera".to_string()));
        assert!(features.contains(&"ar".to_string()));
    }
    
    #[test]
    fn test_version_compare() {
        assert_eq!(version_compare("1.0.0", "1.0.0"), 0);
        assert_eq!(version_compare("1.0.0", "1.0.1"), -1);
        assert_eq!(version_compare("1.1.0", "1.0.1"), 1);
        assert_eq!(version_compare("1.1", "1.1.0"), 0);
        assert_eq!(version_compare("2", "1.9.9"), 1);
    }
    
    #[test]
    fn test_app_config() {
        let mut platform_settings = HashMap::new();
        let mut ios_settings = HashMap::new();
        ios_settings.insert("theme".to_string(), serde_json::json!("light"));
        platform_settings.insert(Platform::iOS, ios_settings);
        
        let mut feature_flags = HashMap::new();
        feature_flags.insert("dark_mode".to_string(), true);
        feature_flags.insert("ar_features".to_string(), false);
        
        let config = AppConfig {
            min_ios_version: "13.0".to_string(),
            min_android_version: "8.0".to_string(),
            feature_flags,
            platform_settings,
        };
        
        let ios_info = DeviceInfo {
            platform: Platform::iOS,
            form_factor: FormFactor::Phone,
            model: "iPhone 11".to_string(),
            os_version: "14.0".to_string(),
            app_version: "1.0.0".to_string(),
            screen_width: 390,
            screen_height: 844,
            pixel_ratio: 2.0,
            language: "en".to_string(),
            timezone: "UTC".to_string(),
            capabilities: DeviceCapabilities::default(),
            is_emulator: false,
            device_id: None,
        };
        
        assert!(config.is_device_supported(&ios_info));
        assert!(config.is_feature_enabled("dark_mode"));
        assert!(!config.is_feature_enabled("ar_features"));
        
        let ios_settings = config.get_platform_settings(Platform::iOS);
        assert_eq!(ios_settings.get("theme").unwrap(), &serde_json::json!("light"));
    }
} 