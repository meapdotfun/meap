//! MEAP Core Protocol Implementation
//! Provides the core functionality for the Message Exchange Agent Protocol

pub mod agent;
pub mod connection;
pub mod protocol;
pub mod security;
pub mod error;

// Re-export commonly used types
pub use agent::{Agent, AgentCapability, AgentStatus};
pub use protocol::{Message, MessageType, Protocol};
pub use error::{Error, Result};
pub use security::{SecurityManager, SecurityConfig, AuthMethod};

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTOCOL_VERSION: &str = "1.0.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_constants() {
        assert!(!VERSION.is_empty());
        assert!(!PROTOCOL_VERSION.is_empty());
    }
}
