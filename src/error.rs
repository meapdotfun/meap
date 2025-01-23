use std::fmt;

#[derive(Debug)]
pub enum MeapError {
    Protocol(String),
    Stream(String),
    Connection(String),
    Validation(String),
    Serialization(String),
}

impl fmt::Display for MeapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeapError::Protocol(msg) => write!(f, "Protocol error: {}", msg),
            MeapError::Stream(msg) => write!(f, "Stream error: {}", msg),
            MeapError::Connection(msg) => write!(f, "Connection error: {}", msg),
            MeapError::Validation(msg) => write!(f, "Validation error: {}", msg),
            MeapError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
        }
    }
}

impl std::error::Error for MeapError {}

pub type Result<T> = std::result::Result<T, MeapError>; 