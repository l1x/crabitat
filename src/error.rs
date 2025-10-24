//! Error types for the Crabitat system

use thiserror::Error;

/// Configuration-related errors
#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    FileRead(String),

    #[error("Failed to parse TOML: {0}")]
    TomlParse(String),

    #[error("Invalid configuration: {0}")]
    Invalid(String),

    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Model-related errors
#[derive(Debug, Error)]
pub enum ModelError {
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    #[error("Invalid response format: {0}")]
    InvalidResponse(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("API error: {0}")]
    ApiError(String),
}

/// System-wide error type
#[derive(Debug, Error)]
pub enum CrabitatError {
    #[error("Config error: {0}")]
    Config(#[from] ConfigError),

    #[error("Model error: {0}")]
    Model(#[from] ModelError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
