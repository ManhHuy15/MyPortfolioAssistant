use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("Validation error on field '{field}': {message}")]
    Validation { field: String, message: String },

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("AI provider error: {0}")]
    AiProvider(String),
}
