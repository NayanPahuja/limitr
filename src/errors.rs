use thiserror::Error;
use tonic::{Code, Status};

#[derive(Error, Debug)]
pub enum RateLimitError {
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Invalid domain: {0}")]
    InvalidDomain(String),

    #[error("Invalid rate configuration: {0}")]
    InvalidRate(String),

    #[error("Redis connection error: {0}")]
    RedisConnectionError(#[from] redis::RedisError),

    #[error("Redis command error: {0}")]
    RedisCommandError(String),

    #[error("Script execution error: {0}")]
    ScriptExecutionError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("File system error: {0}")]
    FileSystemError(#[from] std::io::Error),

    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    InternalError(String),
}

impl From<RateLimitError> for Status {
    fn from(value: RateLimitError) -> Self {
        match value {
            RateLimitError::ConfigurationError(msg) => {
                Status::new(Code::Internal, format!("Configuration Error: {}", msg))
            }
            RateLimitError::InvalidDomain(msg) => {
                Status::new(Code::InvalidArgument, format!("Invalid domain: {}", msg))
            }
            RateLimitError::InvalidRate(msg) => {
                Status::new(Code::InvalidArgument, format!("Invalid rate: {}", msg))
            }
            RateLimitError::RedisConnectionError(err) => Status::new(
                Code::Unavailable,
                format!("Redis connection error: {}", err),
            ),
            RateLimitError::RedisCommandError(msg) => {
                Status::new(Code::Internal, format!("Redis command error: {}", msg))
            }
            RateLimitError::ScriptExecutionError(msg) => {
                Status::new(Code::Internal, format!("Script execution error: {}", msg))
            }
            RateLimitError::SerializationError(msg) => {
                Status::new(Code::Internal, format!("Serialization error: {}", msg))
            }
            RateLimitError::FileSystemError(err) => {
                Status::new(Code::Internal, format!("File system error: {}", err))
            }
            RateLimitError::JsonError(err) => {
                Status::new(Code::Internal, format!("JSON parsing error: {}", err))
            }
            RateLimitError::InternalError(msg) => {
                Status::new(Code::Internal, format!("Internal error: {}", msg))
            }
        }
    }
}


/// Result type alias for rate limiter operations
pub type Result<T> = std::result::Result<T, RateLimitError>;