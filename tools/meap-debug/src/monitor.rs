//! Message monitoring and filtering functionality

use crate::App;
use meap_core::{
    error::Result,
    protocol::{Message, MessageType},
};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info};

/// Message filter criteria
#[derive(Debug, Clone)]
pub struct MessageFilter {
    pub agent_id: Option<String>,
    pub message_type: Option<MessageType>,
    pub content_filter: Option<String>,
}

impl MessageFilter {
    /// Returns true if message matches filter criteria
    pub fn matches(&self, message: &Message) -> bool {
        // Check agent filter
        if let Some(agent) = &self.agent_id {
            if message.from != *agent && message.to != *agent {
                return false;
            }
        }

        // Check message type
        if let Some(msg_type) = &self.message_type {
            if message.message_type != *msg_type {
                return false;
            }
        }

        // Check content filter
        if let Some(filter) = &self.content_filter {
            if !serde_json::to_string(&message.content)
                .unwrap_or_default()
                .contains(filter)
            {
                return false;
            }
        }

        true
    }
}

/// Monitors messages and applies filtering
pub struct MessageMonitor {
    app: Arc<App>,
    filter: MessageFilter,
    tx: broadcast::Sender<Message>,
}

impl MessageMonitor {
    pub fn new(app: Arc<App>) -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            app,
            filter: MessageFilter {
                agent_id: None,
                message_type: None,
                content_filter: None,
            },
            tx,
        }
    }

    /// Sets the current message filter
    pub fn set_filter(&mut self, filter: MessageFilter) {
        self.filter = filter;
    }

    /// Starts monitoring messages
    pub async fn start(&self) -> Result<()> {
        let mut rx = self.tx.subscribe();
        let app = self.app.clone();
        let filter = self.filter.clone();

        tokio::spawn(async move {
            while let Ok(message) = rx.recv().await {
                if filter.matches(&message) {
                    debug!("Received message: {:?}", message);
                    app.add_message(message).await;
                }
            }
        });

        Ok(())
    }

    /// Processes a new message
    pub async fn process_message(&self, message: Message) -> Result<()> {
        if self.filter.matches(&message) {
            if let Err(e) = self.tx.send(message.clone()) {
                error!("Failed to broadcast message: {}", e);
            }
        }
        Ok(())
    }
} 