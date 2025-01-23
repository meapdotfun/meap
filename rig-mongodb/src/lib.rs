use async_trait::async_trait;
use bson::{doc, Document};
use futures::StreamExt;
use meap_core::{
    error::{Error, Result},
    protocol::{Message, MessageType, Protocol},
    agent::{AgentStatus, AgentCapability},
};
use mongodb::{
    Client,
    Collection,
    options::{ClientOptions, FindOptions},
};
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentState {
    pub id: String,
    pub status: AgentStatus,
    pub capabilities: Vec<AgentCapability>,
    pub metadata: serde_json::Value,
    pub last_seen: i64,
}

pub struct MongoStore {
    client: Client,
    agents: Collection<AgentState>,
    messages: Collection<Message>,
}

impl MongoStore {
    pub async fn new(uri: &str, database: &str) -> Result<Self> {
        let options = ClientOptions::parse(uri)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        let client = Client::with_options(options)
            .map_err(|e| Error::Database(e.to_string()))?;

        let db = client.database(database);
        let agents = db.collection("agents");
        let messages = db.collection("messages");

        Ok(Self {
            client,
            agents,
            messages,
        })
    }

    pub async fn save_agent_state(&self, state: AgentState) -> Result<()> {
        self.agents
            .replace_one(
                doc! { "id": &state.id },
                &state,
                mongodb::options::ReplaceOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn get_agent_state(&self, id: &str) -> Result<Option<AgentState>> {
        self.agents
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    pub async fn save_message(&self, message: &Message) -> Result<()> {
        self.messages
            .insert_one(message, None)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn get_messages_for_agent(&self, agent_id: &str, limit: i64) -> Result<Vec<Message>> {
        let options = FindOptions::builder().limit(limit).sort(doc! { "timestamp": -1 }).build();
        
        let cursor = self.messages
            .find(
                doc! { 
                    "$or": [
                        { "from": agent_id },
                        { "to": agent_id }
                    ]
                },
                options,
            )
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        let messages: Vec<Message> = cursor
            .collect()
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(messages)
    }
}

#[async_trait]
impl Protocol for MongoStore {
    async fn validate_message(&self, message: &Message) -> Result<()> {
        match message.message_type {
            MessageType::Request => Ok(()),
            _ => Err(Error::Protocol("Invalid message type for MongoDB store".into())),
        }
    }

    async fn process_message(&self, message: Message) -> Result<Option<Message>> {
        match message.content.get("action").and_then(|v| v.as_str()) {
            Some("save_state") => {
                let state: AgentState = serde_json::from_value(
                    message.content.get("state").unwrap().clone()
                ).map_err(|e| Error::Serialization(e.to_string()))?;
                
                self.save_agent_state(state).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "mongo-store".into(),
                    message.from,
                    serde_json::json!({"status": "success"}),
                )))
            }
            Some("get_state") => {
                let agent_id = message.content.get("agent_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing agent_id".into()))?;
                
                let state = self.get_agent_state(agent_id).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "mongo-store".into(),
                    message.from,
                    serde_json::json!({ "state": state }),
                )))
            }
            Some("get_messages") => {
                let agent_id = message.content.get("agent_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing agent_id".into()))?;
                
                let limit = message.content.get("limit")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(100);
                
                let messages = self.get_messages_for_agent(agent_id, limit).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "mongo-store".into(),
                    message.from,
                    serde_json::json!({ "messages": messages }),
                )))
            }
            _ => Err(Error::Protocol("Unknown MongoDB store action".into())),
        }
    }

    async fn send_message(&self, message: Message) -> Result<()> {
        self.save_message(&message).await
    }

    async fn handle_stream(&self, _message: Message) -> Result<()> {
        Err(Error::Protocol("MongoDB store does not handle streams".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    #[ignore] // Requires MongoDB instance
    async fn test_mongo_store() {
        let store = MongoStore::new("mongodb://localhost:27017", "meap_test")
            .await
            .unwrap();

        // Test saving agent state
        let state = AgentState {
            id: "test1".into(),
            status: AgentStatus::Online,
            capabilities: vec![AgentCapability::Chat],
            metadata: serde_json::json!({"version": "1.0"}),
            last_seen: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        };

        store.save_agent_state(state).await.unwrap();

        // Test retrieving agent state
        let retrieved = store.get_agent_state("test1").await.unwrap().unwrap();
        assert_eq!(retrieved.id, "test1");
    }
} 