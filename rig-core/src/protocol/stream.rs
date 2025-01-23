//! Stream handling for large data transfers

use crate::error::Result;
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Stream control messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamControl {
    /// Start a new stream
    Start {
        /// Stream identifier
        stream_id: String,
        /// Total chunks expected (if known)
        total_chunks: Option<u32>,
        /// Metadata about the stream
        metadata: Option<serde_json::Value>,
    },
    /// Pause the stream
    Pause { stream_id: String },
    /// Resume a paused stream
    Resume { stream_id: String },
    /// End the stream
    End { stream_id: String },
    /// Acknowledge chunk receipt
    Ack { stream_id: String, chunk_id: u32 },
}

/// A chunk of streaming data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Stream identifier
    pub stream_id: String,
    /// Chunk sequence number
    pub chunk_id: u32,
    /// Chunk data
    pub data: Vec<u8>,
    /// Is this the last chunk?
    pub is_last: bool,
}

/// Stream sender for outgoing streams
pub struct StreamSender {
    stream_id: String,
    tx: mpsc::Sender<StreamChunk>,
    chunk_id: u32,
}

impl StreamSender {
    pub fn new(buffer_size: usize) -> (Self, mpsc::Receiver<StreamChunk>) {
        let (tx, rx) = mpsc::channel(buffer_size);
        (
            Self {
                stream_id: Uuid::new_v4().to_string(),
                tx,
                chunk_id: 0,
            },
            rx,
        )
    }

    pub async fn send_chunk(&mut self, data: Vec<u8>, is_last: bool) -> Result<()> {
        let chunk = StreamChunk {
            stream_id: self.stream_id.clone(),
            chunk_id: self.chunk_id,
            data,
            is_last,
        };
        self.chunk_id += 1;
        self.tx.send(chunk).await
            .map_err(|e| crate::error::Error::Stream(format!("Failed to send chunk: {}", e)))
    }
}

/// Stream receiver for incoming streams
pub struct StreamReceiver {
    stream_id: String,
    rx: mpsc::Receiver<StreamChunk>,
    buffer_size: usize,
}

impl StreamReceiver {
    pub fn new(stream_id: String, buffer_size: usize) -> (Self, mpsc::Sender<StreamChunk>) {
        let (tx, rx) = mpsc::channel(buffer_size);
        (
            Self {
                stream_id,
                rx,
                buffer_size,
            },
            tx,
        )
    }

    pub fn into_stream(self) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>> {
        Box::pin(async_stream::stream! {
            let mut rx = self.rx;
            while let Some(chunk) = rx.recv().await {
                yield Ok(chunk);
            }
        })
    }
} 