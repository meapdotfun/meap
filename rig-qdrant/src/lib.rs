use async_trait::async_trait;
use meap_core::{
    error::{Error, Result},
    protocol::{Message, MessageType, Protocol},
};
use qdrant_client::{
    prelude::*,
    qdrant::{
        vectors_config::Config,
        VectorParams,
        VectorsConfig,
        Distance,
        PointStruct,
        SearchPoints,
        WithPayloadSelector,
        WithVectorsSelector,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error};
use uuid::Uuid;

const DEFAULT_VECTOR_SIZE: u64 = 1536; // OpenAI ada-002 embedding size

#[derive(Debug, Serialize, Deserialize)]
pub struct VectorPoint {
    pub id: String,
    pub vector: Vec<f32>,
    pub metadata: HashMap<String, String>,
}

pub struct QdrantStore {
    client: QdrantClient,
    collection_name: String,
}

impl QdrantStore {
    pub async fn new(url: &str, collection_name: &str) -> Result<Self> {
        let config = QdrantClientConfig::from_url(url);
        let client = QdrantClient::new(Some(config))
            .map_err(|e| Error::Database(e.to_string()))?;

        let store = Self {
            client,
            collection_name: collection_name.to_string(),
        };

        // Ensure collection exists
        store.create_collection_if_not_exists().await?;
        Ok(store)
    }

    async fn create_collection_if_not_exists(&self) -> Result<()> {
        let collections = self.client
            .list_collections()
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        if !collections.collections
            .iter()
            .any(|c| c.name == self.collection_name)
        {
            let vectors_config = VectorsConfig {
                config: Some(Config::Params(VectorParams {
                    size: DEFAULT_VECTOR_SIZE,
                    distance: Distance::Cosine.into(),
                    ..Default::default()
                })),
            };

            self.client
                .create_collection(&CreateCollection {
                    collection_name: self.collection_name.clone(),
                    vectors_config: Some(vectors_config),
                    ..Default::default()
                })
                .await
                .map_err(|e| Error::Database(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn upsert_point(&self, point: VectorPoint) -> Result<()> {
        let point_id = point.id.parse()
            .map_err(|e| Error::Database(format!("Invalid point ID: {}", e)))?;

        let payload: Payload = point.metadata.into();
        
        let point_struct = PointStruct {
            id: Some(point_id.into()),
            vectors: Some(point.vector.into()),
            payload: Some(payload),
        };

        self.client
            .upsert_points(&self.collection_name, None, vec![point_struct], None)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn search(&self, query_vector: Vec<f32>, limit: u64) -> Result<Vec<VectorPoint>> {
        let search_points = SearchPoints {
            collection_name: self.collection_name.clone(),
            vector: query_vector,
            limit,
            with_payload: Some(WithPayloadSelector::from(true)),
            with_vectors: Some(WithVectorsSelector::from(true)),
            ..Default::default()
        };

        let results = self.client
            .search_points(&search_points)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        let points = results
            .into_iter()
            .filter_map(|point| {
                let id = point.id?.to_string();
                let vector = point.vectors?.into();
                let metadata = point.payload?
                    .into_iter()
                    .map(|(k, v)| (k, v.to_string()))
                    .collect();

                Some(VectorPoint {
                    id,
                    vector,
                    metadata,
                })
            })
            .collect();

        Ok(points)
    }

    pub async fn delete_point(&self, id: &str) -> Result<()> {
        let point_id = id.parse()
            .map_err(|e| Error::Database(format!("Invalid point ID: {}", e)))?;

        self.client
            .delete_points(
                &self.collection_name,
                &PointsSelector::from(vec![point_id]),
                None,
            )
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl Protocol for QdrantStore {
    async fn validate_message(&self, message: &Message) -> Result<()> {
        match message.message_type {
            MessageType::Request => Ok(()),
            _ => Err(Error::Protocol("Invalid message type for Qdrant store".into())),
        }
    }

    async fn process_message(&self, message: Message) -> Result<Option<Message>> {
        match message.content.get("action").and_then(|v| v.as_str()) {
            Some("upsert") => {
                let point: VectorPoint = serde_json::from_value(
                    message.content.get("point").unwrap().clone()
                ).map_err(|e| Error::Serialization(e.to_string()))?;
                
                self.upsert_point(point).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "qdrant-store".into(),
                    message.from,
                    serde_json::json!({"status": "success"}),
                )))
            }
            Some("search") => {
                let query: Vec<f32> = serde_json::from_value(
                    message.content.get("vector").unwrap().clone()
                ).map_err(|e| Error::Serialization(e.to_string()))?;
                
                let limit = message.content.get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10);
                
                let results = self.search(query, limit).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "qdrant-store".into(),
                    message.from,
                    serde_json::json!({ "results": results }),
                )))
            }
            Some("delete") => {
                let id = message.content.get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing point ID".into()))?;

                self.delete_point(id).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "qdrant-store".into(),
                    message.from,
                    serde_json::json!({"status": "success"}),
                )))
            }
            _ => Err(Error::Protocol("Unknown Qdrant store action".into())),
        }
    }

    async fn send_message(&self, _message: Message) -> Result<()> {
        Err(Error::Protocol("Qdrant store does not send messages".into()))
    }

    async fn handle_stream(&self, _message: Message) -> Result<()> {
        Err(Error::Protocol("Qdrant store does not handle streams".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires Qdrant instance
    async fn test_qdrant_store() {
        let store = QdrantStore::new("http://localhost:6334", "test_collection")
            .await
            .unwrap();

        // Test upserting point
        let point = VectorPoint {
            id: Uuid::new_v4().to_string(),
            vector: vec![0.1; DEFAULT_VECTOR_SIZE as usize],
            metadata: [("key".to_string(), "value".to_string())].into(),
        };

        store.upsert_point(point.clone()).await.unwrap();

        // Test searching
        let results = store.search(vec![0.1; DEFAULT_VECTOR_SIZE as usize], 1)
            .await
            .unwrap();
        
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, point.id);

        // Test deleting
        store.delete_point(&point.id).await.unwrap();
    }
} 