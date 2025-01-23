//! Application state management

use crate::monitor::{MessageFilter, MessageMonitor};
use crate::search::MessageSearch;
use meap_core::{
    connection::ConnectionPool,
    protocol::Message,
    error::Result,
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Application state
pub struct AppState {
    pub connection_pool: Arc<ConnectionPool>,
    pub messages: Arc<RwLock<Vec<Message>>>,
    pub selected_agent: Option<String>,
    pub show_help: bool,
    pub show_filter_menu: bool,
    pub filter: MessageFilter,
    pub monitor: Option<MessageMonitor>,
    pub show_search: bool,
    pub search_input: String,
}

impl AppState {
    pub fn new(connection_pool: Arc<ConnectionPool>) -> Self {
        Self {
            connection_pool,
            messages: Arc::new(RwLock::new(Vec::new())),
            selected_agent: None,
            show_help: false,
            show_filter_menu: false,
            filter: MessageFilter {
                agent_id: None,
                message_type: None,
                content_filter: None,
            },
            monitor: None,
            show_search: false,
            search_input: String::new(),
        }
    }

    pub async fn add_message(&self, message: Message) {
        let mut messages = self.messages.write().await;
        messages.push(message);
        if messages.len() > 100 {
            messages.remove(0);
        }
    }

    pub fn set_filter(&self, filter: MessageFilter) {
        if let Some(monitor) = &self.monitor {
            monitor.set_filter(filter);
        }
    }
}

/// Main application with UI state and functionality
pub struct App {
    pub state: Arc<AppState>,
    pub search: Arc<MessageSearch>,
}

impl App {
    pub fn new(connection_pool: Arc<ConnectionPool>) -> Self {
        let state = Arc::new(AppState::new(connection_pool));
        let search = Arc::new(MessageSearch::new(state.clone()));

        Self { state, search }
    }
} 