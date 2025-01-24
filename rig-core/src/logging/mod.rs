use std::{
    collections::HashMap,
    fmt,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use serde::Serialize;
use tokio::sync::broadcast;
use tracing::{Event, Subscriber, field::{Field, Visit}};
use tracing_subscriber::Layer;

/// Log levels with color support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LogLevel {
    Error,   // Red
    Warn,    // Yellow  
    Info,    // Blue
    Debug,   // Green
    Trace,   // Gray
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let colored = match self {
            LogLevel::Error => "\x1b[31mERROR\x1b[0m",
            LogLevel::Warn => "\x1b[33mWARN\x1b[0m",
            LogLevel::Info => "\x1b[34mINFO\x1b[0m", 
            LogLevel::Debug => "\x1b[32mDEBUG\x1b[0m",
            LogLevel::Trace => "\x1b[90mTRACE\x1b[0m",
        };
        write!(f, "{}", colored)
    }
}

/// Structured log entry
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    timestamp: u64,
    level: LogLevel,
    target: String,
    message: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    fields: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl LogEntry {
    pub fn new(level: LogLevel, target: String, message: String) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            level,
            target,
            message,
            fields: HashMap::new(),
            error: None,
        }
    }

    pub fn add_field<T: Serialize>(&mut self, key: String, value: T) {
        if let Ok(value) = serde_json::to_value(value) {
            self.fields.insert(key, value);
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }
}

/// Custom field visitor for tracing events
struct FieldVisitor<'a> {
    entry: &'a mut LogEntry,
}

impl<'a> Visit for FieldVisitor<'a> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.entry.add_field(
            field.name().to_string(),
            format!("{:?}", value)
        );
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.entry.add_field(field.name().to_string(), value);
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        if field.name() == "message" {
            self.entry.set_error(value.to_string());
        } else {
            self.entry.add_field(field.name().to_string(), value.to_string());
        }
    }
}

/// Custom logging layer for tracing
pub struct MeapLogger {
    tx: broadcast::Sender<LogEntry>,
}

impl MeapLogger {
    pub fn new(buffer: usize) -> (Self, broadcast::Receiver<LogEntry>) {
        let (tx, rx) = broadcast::channel(buffer);
        (Self { tx }, rx)
    }
}

impl<S: Subscriber> Layer<S> for MeapLogger {
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let metadata = event.metadata();
        
        let level = match *metadata.level() {
            tracing::Level::ERROR => LogLevel::Error,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::DEBUG => LogLevel::Debug,
            tracing::Level::TRACE => LogLevel::Trace,
        };

        let mut entry = LogEntry::new(
            level,
            metadata.target().to_string(),
            metadata.name().to_string(),
        );

        let mut visitor = FieldVisitor { entry: &mut entry };
        event.record(&mut visitor);

        // Broadcast log entry
        let _ = self.tx.send(entry);
    }
}

/// Log collector for aggregating and filtering logs
pub struct LogCollector {
    rx: broadcast::Receiver<LogEntry>,
    entries: Arc<tokio::sync::RwLock<Vec<LogEntry>>>,
    max_entries: usize,
}

impl LogCollector {
    pub fn new(rx: broadcast::Receiver<LogEntry>, max_entries: usize) -> Self {
        Self {
            rx,
            entries: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            max_entries,
        }
    }

    pub async fn start(&mut self) {
        while let Ok(entry) = self.rx.recv().await {
            let mut entries = self.entries.write().await;
            entries.push(entry);
            
            while entries.len() > self.max_entries {
                entries.remove(0);
            }
        }
    }

    pub async fn get_entries(&self, level: Option<LogLevel>) -> Vec<LogEntry> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .filter(|e| level.map_or(true, |l| e.level == l))
            .cloned()
            .collect()
    }
} 