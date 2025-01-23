use async_trait::async_trait;
use lancedb::{Connection, Table};
use meap_core::{
    error::{Error, Result},
    protocol::{Message, MessageType, Protocol},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, error};

#[derive(Debug, Serialize, Deserialize)]
pub struct VectorData {
    pub id: String,
    pub vector: Vec<f32>,
    pub metadata: serde_json::Value,
}

pub struct LanceDBStore {
    conn: Connection,
    table: Table,
}

impl LanceDBStore {
    pub async fn new(path: PathBuf, table_name: &str) -> Result<Self> {
        let conn = lancedb::connect(path)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        let table = conn
            .open_table(table_name)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(Self { conn, table })
    }

    pub async fn add_vector(&self, data: VectorData) -> Result<()> {
        self.table
            .add(vec![data])
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn search(&self, query: Vec<f32>, limit: usize) -> Result<Vec<VectorData>> {
        let results = self.table
            .search()
            .vector(query)
            .limit(limit)
            .execute()
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        // Convert results to VectorData
        let vector_data = results
            .into_iter()
            .map(|row| VectorData {
                id: row.get("id").unwrap_or_default(),
                vector: row.get("vector").unwrap_or_default(),
                metadata: row.get("metadata").unwrap_or_default(),
            })
            .collect();

        Ok(vector_data)
    }
}

#[async_trait]
impl Protocol for LanceDBStore {
    async fn validate_message(&self, message: &Message) -> Result<()> {
        match message.message_type {
            MessageType::Request => Ok(()),
            _ => Err(Error::Protocol("Invalid message type for vector store".into())),
        }
    }

    async fn process_message(&self, message: Message) -> Result<Option<Message>> {
        match message.content.get("action").and_then(|v| v.as_str()) {
            Some("add") => {
                let data: VectorData = serde_json::from_value(
                    message.content.get("data").unwrap().clone()
                ).map_err(|e| Error::Serialization(e.to_string()))?;
                
                self.add_vector(data).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "vector-store".into(),
                    message.from,
                    serde_json::json!({"status": "success"}),
                )))
            }
            Some("search") => {
                let query: Vec<f32> = serde_json::from_value(
                    message.content.get("query").unwrap().clone()
                ).map_err(|e| Error::Serialization(e.to_string()))?;
                
                let limit = message.content.get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as usize;
                
                let results = self.search(query, limit).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "vector-store".into(),
                    message.from,
                    serde_json::json!({ "results": results }),
                )))
            }
            _ => Err(Error::Protocol("Unknown vector store action".into())),
        }
    }

    async fn send_message(&self, _message: Message) -> Result<()> {
        Err(Error::Protocol("Vector store does not send messages".into()))
    }

    async fn handle_stream(&self, _message: Message) -> Result<()> {
        Err(Error::Protocol("Vector store does not handle streams".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_vector_store() {
        let dir = tempdir().unwrap();
        let store = LanceDBStore::new(dir.path().to_path_buf(), "test")
            .await
            .unwrap();

        // Test adding a vector
        let data = VectorData {
            id: "test1".into(),
            vector: vec![1.0, 2.0, 3.0],
            metadata: serde_json::json!({"label": "test"}),
        };

        store.add_vector(data).await.unwrap();

        // Test searching
        let results = store.search(vec![1.0, 2.0, 3.0], 1).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "test1");
    }
} 