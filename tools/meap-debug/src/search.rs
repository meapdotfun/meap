//! Message search functionality

use crate::App;
use meap_core::{
    error::Result,
    protocol::Message,
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Search criteria for messages
#[derive(Debug, Clone)]
pub struct SearchCriteria {
    /// Search term to match against message content
    pub term: String,
    /// Whether to search case-sensitively
    pub case_sensitive: bool,
    /// Maximum number of results to return
    pub limit: usize,
}

/// Search results with matched messages
#[derive(Debug)]
pub struct SearchResults {
    pub matches: Vec<Message>,
    pub total_matches: usize,
}

/// Handles message searching
pub struct MessageSearch {
    app: Arc<App>,
    results: Arc<RwLock<Option<SearchResults>>>,
}

impl MessageSearch {
    pub fn new(app: Arc<App>) -> Self {
        Self {
            app,
            results: Arc::new(RwLock::new(None)),
        }
    }

    /// Performs a search using the given criteria
    pub async fn search(&self, criteria: SearchCriteria) -> Result<()> {
        let messages = self.app.messages.read().await;
        let mut matches = Vec::new();
        let search_term = if criteria.case_sensitive {
            criteria.term.clone()
        } else {
            criteria.term.to_lowercase()
        };

        for message in messages.iter() {
            let content = serde_json::to_string(&message.content)
                .unwrap_or_default();
            let content = if criteria.case_sensitive {
                content
            } else {
                content.to_lowercase()
            };

            if content.contains(&search_term) {
                matches.push(message.clone());
                if matches.len() >= criteria.limit {
                    break;
                }
            }
        }

        let total_matches = matches.len();
        let results = SearchResults {
            matches,
            total_matches,
        };

        *self.results.write().await = Some(results);
        Ok(())
    }

    /// Gets the current search results
    pub async fn get_results(&self) -> Option<SearchResults> {
        self.results.read().await.clone()
    }

    /// Clears the current search results
    pub async fn clear_results(&self) {
        *self.results.write().await = None;
    }
} 