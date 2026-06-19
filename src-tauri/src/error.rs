//! Application-wide error type.
//!
//! `AppError` implements `Serialize` (as a plain string) so it can be returned
//! straight out of `#[tauri::command]` functions and surfaced to the frontend.

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Docker engine error: {0}")]
    Docker(#[from] bollard::errors::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Tauri error: {0}")]
    Tauri(#[from] tauri::Error),

    #[error("WSL is not available: {0}")]
    WslUnavailable(String),

    #[error("setup failed: {0}")]
    Setup(String),

    #[error("{0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Convenience alias used throughout the backend.
pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn other(msg: impl Into<String>) -> Self {
        AppError::Other(msg.into())
    }
    pub fn setup(msg: impl Into<String>) -> Self {
        AppError::Setup(msg.into())
    }
}
