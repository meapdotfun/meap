use crate::{Message, MessageType};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StreamChunk {
    pub index: u32,
    pub data: Vec<u8>,
    pub final_chunk: bool,
}

pub struct StreamHandler {
    buffer: VecDeque<StreamChunk>,
    next_index: u32,
    agent_id: String,
}

impl StreamHandler {
    pub fn new(agent_id: String) -> Self {
        Self {
            buffer: VecDeque::new(),
            next_index: 0,
            agent_id,
        }
    }

    pub fn create_stream_message(&mut self, to: String, data: Vec<u8>, chunk_size: usize) -> Vec<Message> {
        let mut messages = Vec::new();
        let chunks = data.chunks(chunk_size);
        let total_chunks = chunks.len();

        for (i, chunk) in chunks.enumerate() {
            let stream_chunk = StreamChunk {
                index: self.next_index,
                data: chunk.to_vec(),
                final_chunk: i == total_chunks - 1,
            };

            let message = Message {
                id: uuid::Uuid::new_v4().to_string(),
                message_type: MessageType::Stream,
                from: self.agent_id.clone(),
                to: to.clone(),
                content: serde_json::to_value(stream_chunk).unwrap(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };

            messages.push(message);
            self.next_index += 1;
        }

        messages
    }

    pub fn process_stream_chunk(&mut self, chunk: StreamChunk) -> Option<Vec<u8>> {
        self.buffer.push_back(chunk);
        
        // Check if we have all chunks up to a final one
        if let Some(final_chunk) = self.buffer.iter().find(|c| c.final_chunk) {
            let final_index = final_chunk.index;
            
            // Check if we have all chunks up to the final one
            if (0..=final_index).all(|i| self.buffer.iter().any(|c| c.index == i)) {
                // Collect and order all chunks
                let mut data = Vec::new();
                let mut chunks: Vec<_> = self.buffer.drain(..).collect();
                chunks.sort_by_key(|c| c.index);
                
                for chunk in chunks {
                    data.extend(chunk.data);
                }
                
                return Some(data);
            }
        }
        
        None
    }
} 