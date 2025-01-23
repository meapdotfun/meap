use thiserror::Error;
use std::io;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Protocol error: {0}")]
    Protocol(String),
    
    #[error("Connection error: {0}")]
    Connection(String),
    
    #[error("Send error: {0}")]
    Send(String),
    
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Security error: {0}")]
    Security(String),
    
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Stream error: {0}")]
    Stream(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Agent error: {0}")]
    Agent(String),

    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn protocol(msg: impl Into<String>) -> Self {
        Error::Protocol(msg.into())
    }

    pub fn security(msg: impl Into<String>) -> Self {
        Error::Security(msg.into())
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        Error::Validation(msg.into())
    }
}
