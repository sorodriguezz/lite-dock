//! Application-wide error type.
//!
//! `AppError` implements `Serialize` (as a plain string) so it can be returned
//! straight out of `#[tauri::command]` functions and surfaced to the frontend.

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Error del motor Docker: {0}")]
    Docker(#[from] bollard::errors::Error),

    #[error("Error de E/S: {0}")]
    Io(#[from] std::io::Error),

    #[error("Error de serialización: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Error de Tauri: {0}")]
    Tauri(#[from] tauri::Error),

    #[error("WSL no está disponible: {0}")]
    WslUnavailable(String),

    #[error("Falló la instalación: {0}")]
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
