use async_nats::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{info, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub topic: String,
    pub payload: Vec<u8>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Broker {
    client: Client,
    subscriptions: RwLock<HashMap<String, Vec<String>>>,
}

impl Broker {
    pub async fn new(url: &str) -> anyhow::Result<Self> {
        let client = async_nats::connect(url).await?;
        Ok(Self {
            client,
            subscriptions: RwLock::new(HashMap::new()),
        })
    }

    pub async fn publish(&self, message: Message) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(&message)?;
        self.client.publish(message.topic, payload.into()).await?;
        info!("Published message to topic: {}", message.topic);
        Ok(())
    }

    pub async fn subscribe(&self, topic: &str, queue: &str) -> anyhow::Result<()> {
        let mut subs = self.subscriptions.write().await;
        let subscribers = subs.entry(topic.to_string()).or_insert_with(Vec::new);
        subscribers.push(queue.to_string());
        
        let subscription = self.client.queue_subscribe(topic, queue).await?;
        
        tokio::spawn(async move {
            while let Some(msg) = subscription.next().await {
                if let Ok(message) = serde_json::from_slice::<Message>(&msg.payload) {
                    info!("Received message on topic {}: {:?}", topic, message);
                } else {
                    error!("Failed to deserialize message on topic {}", topic);
                }
            }
        });

        Ok(())
    }

    pub async fn get_subscribers(&self, topic: &str) -> Vec<String> {
        let subs = self.subscriptions.read().await;
        subs.get(topic).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_broker_pub_sub() {
        let broker = Broker::new("nats://localhost:4222").await.unwrap();
        
        // Subscribe to a topic
        broker.subscribe("test.topic", "test.queue").await.unwrap();
        
        // Publish a message
        let message = Message {
            topic: "test.topic".to_string(),
            payload: b"test message".to_vec(),
            metadata: HashMap::new(),
        };
        
        broker.publish(message).await.unwrap();
        
        // Verify subscribers
        let subscribers = broker.get_subscribers("test.topic").await;
        assert!(subscribers.contains(&"test.queue".to_string()));
    }
} 