use async_trait::async_trait;
use meap_core::{
    error::{Error, Result},
    protocol::{Message, MessageType, Protocol},
    agent::{AgentStatus, AgentCapability},
};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePool, Row};
use std::path::Path;
use tracing::{debug, error};

#[derive(Debug, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub message_type: String,
    pub from_agent: String,
    pub to_agent: String,
    pub content: String,
    pub timestamp: i64,
}

pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let pool = SqlitePool::connect(&format!("sqlite:{}", path.as_ref().display()))
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        // Initialize tables
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                message_type TEXT NOT NULL,
                from_agent TEXT NOT NULL,
                to_agent TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agents (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                capabilities TEXT NOT NULL,
                metadata TEXT NOT NULL,
                last_seen INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        Ok(Self { pool })
    }

    pub async fn save_message(&self, message: &Message) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO messages (id, message_type, from_agent, to_agent, content, timestamp)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(&message.id)
        .bind(format!("{:?}", message.message_type))
        .bind(&message.from)
        .bind(&message.to)
        .bind(serde_json::to_string(&message.content).unwrap())
        .bind(message.timestamp as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn get_messages(&self, agent_id: &str, limit: i64) -> Result<Vec<Message>> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM messages 
            WHERE from_agent = $1 OR to_agent = $1
            ORDER BY timestamp DESC
            LIMIT $2
            "#,
        )
        .bind(agent_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        let messages = rows
            .iter()
            .map(|row| {
                let content: String = row.get("content");
                Message {
                    id: row.get("id"),
                    message_type: serde_json::from_str(&row.get::<String, _>("message_type")).unwrap(),
                    from: row.get("from_agent"),
                    to: row.get("to_agent"),
                    content: serde_json::from_str(&content).unwrap(),
                    timestamp: row.get::<i64, _>("timestamp") as u64,
                }
            })
            .collect();

        Ok(messages)
    }

    pub async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO config (key, value)
            VALUES ($1, $2)
            "#,
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn get_config(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM config WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(row.map(|r| r.get("value")))
    }
}

#[async_trait]
impl Protocol for SqliteStore {
    async fn validate_message(&self, message: &Message) -> Result<()> {
        match message.message_type {
            MessageType::Request => Ok(()),
            _ => Err(Error::Protocol("Invalid message type for SQLite store".into())),
        }
    }

    async fn process_message(&self, message: Message) -> Result<Option<Message>> {
        match message.content.get("action").and_then(|v| v.as_str()) {
            Some("save_message") => {
                self.save_message(&message).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "sqlite-store".into(),
                    message.from,
                    serde_json::json!({"status": "success"}),
                )))
            }
            Some("get_messages") => {
                let agent_id = message.content.get("agent_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing agent_id".into()))?;

                let limit = message.content.get("limit")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(100);

                let messages = self.get_messages(agent_id, limit).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "sqlite-store".into(),
                    message.from,
                    serde_json::json!({ "messages": messages }),
                )))
            }
            Some("set_config") => {
                let key = message.content.get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing key".into()))?;

                let value = message.content.get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing value".into()))?;

                self.set_config(key, value).await?;
                Ok(Some(Message::new(
                    MessageType::Response,
                    "sqlite-store".into(),
                    message.from,
                    serde_json::json!({"status": "success"}),
                )))
            }
            _ => Err(Error::Protocol("Unknown SQLite store action".into())),
        }
    }

    async fn send_message(&self, message: Message) -> Result<()> {
        self.save_message(&message).await
    }

    async fn handle_stream(&self, _message: Message) -> Result<()> {
        Err(Error::Protocol("SQLite store does not handle streams".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_sqlite_store() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = SqliteStore::new(temp_file.path()).await.unwrap();

        // Test saving message
        let message = Message::new(
            MessageType::Request,
            "agent1".into(),
            "agent2".into(),
            serde_json::json!({"test": "data"}),
        );

        store.save_message(&message).await.unwrap();

        // Test retrieving messages
        let messages = store.get_messages("agent1", 10).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from, "agent1");

        // Test config
        store.set_config("test_key", "test_value").await.unwrap();
        let value = store.get_config("test_key").await.unwrap();
        assert_eq!(value, Some("test_value".to_string()));
    }
} 