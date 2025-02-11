//! Stream handling for Deepseek API responses
//! Manages streaming responses for long-running operations

use crate::{
    error::{DeepseekError, Result},
    response::{GenerateResponse, TextResponse},
};
use futures::{Stream, StreamExt};
use serde::Deserialize;
use std::pin::Pin;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Chunk of a streaming response
#[derive(Debug, Deserialize)]
pub struct StreamChunk {
    /// Content of this chunk
    pub content: String,
    /// Is this the final chunk?
    pub is_final: bool,
    /// Optional metadata for this chunk
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Stream processor for handling chunked responses
pub struct StreamProcessor {
    buffer_size: usize,
    chunk_handler: Box<dyn ChunkHandler + Send>,
}

/// Trait for handling stream chunks
#[async_trait::async_trait]
pub trait ChunkHandler: Send {
    async fn handle_chunk(&mut self, chunk: StreamChunk) -> Result<()>;
    async fn finalize(&mut self) -> Result<()>;
}

impl StreamProcessor {
    pub fn new(buffer_size: usize, handler: impl ChunkHandler + Send + 'static) -> Self {
        Self {
            buffer_size,
            chunk_handler: Box::new(handler),
        }
    }

    /// Processes a stream of chunks
    pub async fn process_stream<S>(&mut self, stream: S) -> Result<()>
    where
        S: Stream<Item = Result<StreamChunk>> + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel(self.buffer_size);
        
        // Spawn chunk processing task
        let mut handler = std::mem::replace(
            &mut self.chunk_handler, 
            Box::new(NullHandler)
        );
        
        tokio::spawn(async move {
            while let Some(chunk) = rx.recv().await {
                if let Err(e) = handler.handle_chunk(chunk).await {
                    error!("Error handling chunk: {}", e);
                }
            }
            if let Err(e) = handler.finalize().await {
                error!("Error finalizing stream: {}", e);
            }
        });

        // Process incoming stream
        tokio::pin!(stream);
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    if let Err(e) = tx.send(chunk).await {
                        error!("Failed to send chunk: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Stream error: {}", e);
                    return Err(DeepseekError::ConnectionError(e.to_string()));
                }
            }
        }

        Ok(())
    }
}

/// Code generation stream handler
pub struct CodeGenHandler {
    accumulated: String,
    callback: Box<dyn Fn(&str) + Send>,
}

impl CodeGenHandler {
    pub fn new(callback: impl Fn(&str) + Send + 'static) -> Self {
        Self {
            accumulated: String::new(),
            callback: Box::new(callback),
        }
    }
}

#[async_trait::async_trait]
impl ChunkHandler for CodeGenHandler {
    async fn handle_chunk(&mut self, chunk: StreamChunk) -> Result<()> {
        self.accumulated.push_str(&chunk.content);
        (self.callback)(&self.accumulated);
        Ok(())
    }

    async fn finalize(&mut self) -> Result<()> {
        debug!("Code generation stream complete");
        Ok(())
    }
}

/// Null handler for placeholder usage
struct NullHandler;

#[async_trait::async_trait]
impl ChunkHandler for NullHandler {
    async fn handle_chunk(&mut self, _: StreamChunk) -> Result<()> {
        Ok(())
    }
    
    async fn finalize(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn test_stream_processing() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();

        let handler = CodeGenHandler::new(move |code| {
            let mut received = received_clone.blocking_lock();
            received.push(code.to_string());
        });

        let mut processor = StreamProcessor::new(32, handler);

        let chunks = vec![
            Ok(StreamChunk {
                content: "fn ".into(),
                is_final: false,
                metadata: serde_json::json!({}),
            }),
            Ok(StreamChunk {
                content: "main".into(),
                is_final: false,
                metadata: serde_json::json!({}),
            }),
            Ok(StreamChunk {
                content: "() {}".into(),
                is_final: true,
                metadata: serde_json::json!({}),
            }),
        ];

        processor.process_stream(stream::iter(chunks)).await.unwrap();

        let received = received.lock().await;
        assert_eq!(received.len(), 3);
        assert_eq!(received.last().unwrap(), "fn main() {}");
    }
} 