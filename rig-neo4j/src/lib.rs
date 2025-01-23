use async_trait::async_trait;
use meap_core::{
    error::{Error, Result},
    protocol::{Message, MessageType, Protocol},
};
use neo4rs::{Graph, Node, query};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error};

#[derive(Debug, Serialize, Deserialize)]
pub struct Relationship {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub properties: HashMap<String, serde_json::Value>,
}

pub struct Neo4jStore {
    graph: Graph,
}

impl Neo4jStore {
    pub async fn new(uri: &str, user: &str, password: &str) -> Result<Self> {
        let graph = Graph::new(uri, user, password)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(Self { graph })
    }

    pub async fn create_agent_node(&self, id: &str, properties: HashMap<String, serde_json::Value>) -> Result<()> {
        let props: HashMap<String, String> = properties.into_iter()
            .map(|(k, v)| (k, v.to_string()))
            .collect();

        let query = query("CREATE (a:Agent {id: $id}) SET a += $props")
            .param("id", id)
            .param("props", props);

        self.graph
            .run(query)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn create_relationship(&self, relationship: Relationship) -> Result<()> {
        let props: HashMap<String, String> = relationship.properties.into_iter()
            .map(|(k, v)| (k, v.to_string()))
            .collect();

        let query = query(
            "MATCH (a:Agent {id: $from}), (b:Agent {id: $to}) 
             CREATE (a)-[r:RELATES {kind: $kind}]->(b) 
             SET r += $props"
        )
        .param("from", relationship.from)
        .param("to", relationship.to)
        .param("kind", relationship.kind)
        .param("props", props);

        self.graph
            .run(query)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn get_relationships(&self, agent_id: &str) -> Result<Vec<Relationship>> {
        let query = query(
            "MATCH (a:Agent {id: $id})-[r:RELATES]->(b:Agent) 
             RETURN r, b.id as to"
        )
        .param("id", agent_id);

        let mut result = self.graph
            .run(query)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        let mut relationships = Vec::new();

        while let Ok(Some(row)) = result.next().await {
            let rel: Node = row.get("r").unwrap();
            let to: String = row.get("to").unwrap();
            
            let mut properties = HashMap::new();
            for (key, value) in rel.properties().into_iter() {
                properties.insert(
                    key.to_string(),
                    serde_json::Value::String(value.to_string()),
                );
            }

            relationships.push(Relationship {
                from: agent_id.to_string(),
                to,
                kind: rel.get("kind").unwrap_or_default(),
                properties,
            });
        }

        Ok(relationships)
    }
}

#[async_trait]
impl Protocol for Neo4jStore {
    async fn validate_message(&self, message: &Message) -> Result<()> {
        match message.message_type {
            MessageType::Request => Ok(()),
            _ => Err(Error::Protocol("Invalid message type for Neo4j store".into())),
        }
    }

    async fn process_message(&self, message: Message) -> Result<Option<Message>> {
        match message.content.get("action").and_then(|v| v.as_str()) {
            Some("create_agent") => {
                let id = message.content.get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing agent id".into()))?;

                let properties = message.content.get("properties")
                    .and_then(|v| v.as_object())
                    .map(|obj| obj.clone())
                    .unwrap_or_default();

                self.create_agent_node(id, properties).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "neo4j-store".into(),
                    message.from,
                    serde_json::json!({"status": "success"}),
                )))
            }
            Some("create_relationship") => {
                let relationship: Relationship = serde_json::from_value(
                    message.content.get("relationship").unwrap().clone()
                ).map_err(|e| Error::Serialization(e.to_string()))?;

                self.create_relationship(relationship).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "neo4j-store".into(),
                    message.from,
                    serde_json::json!({"status": "success"}),
                )))
            }
            Some("get_relationships") => {
                let agent_id = message.content.get("agent_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing agent_id".into()))?;

                let relationships = self.get_relationships(agent_id).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "neo4j-store".into(),
                    message.from,
                    serde_json::json!({ "relationships": relationships }),
                )))
            }
            _ => Err(Error::Protocol("Unknown Neo4j store action".into())),
        }
    }

    async fn send_message(&self, _message: Message) -> Result<()> {
        Err(Error::Protocol("Neo4j store does not send messages".into()))
    }

    async fn handle_stream(&self, _message: Message) -> Result<()> {
        Err(Error::Protocol("Neo4j store does not handle streams".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires Neo4j instance
    async fn test_neo4j_store() {
        let store = Neo4jStore::new(
            "neo4j://localhost:7687",
            "neo4j",
            "password",
        ).await.unwrap();

        // Test creating agent node
        let mut properties = HashMap::new();
        properties.insert("name".to_string(), serde_json::json!("Test Agent"));
        
        store.create_agent_node("test1", properties).await.unwrap();

        // Test creating relationship
        let relationship = Relationship {
            from: "test1".into(),
            to: "test2".into(),
            kind: "KNOWS".into(),
            properties: HashMap::new(),
        };

        store.create_relationship(relationship).await.unwrap();

        // Test getting relationships
        let relationships = store.get_relationships("test1").await.unwrap();
        assert_eq!(relationships.len(), 1);
        assert_eq!(relationships[0].kind, "KNOWS");
    }
} 