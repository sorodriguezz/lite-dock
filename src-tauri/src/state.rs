//! Shared application state managed by Tauri.

use bollard::Docker;
use std::collections::HashMap;
use std::pin::Pin;
use tokio::io::AsyncWrite;
use tokio::sync::Mutex;

pub struct AppState {
    /// Cached bollard client. Lazily created on first use.
    pub docker: Mutex<Option<Docker>>,
    /// The foreground `wsl … litedock-init.sh` process (dockerd). Present while
    /// the engine is running; terminated on shutdown.
    pub engine_child: Mutex<Option<tokio::process::Child>>,
    /// Active interactive exec sessions: session id -> stdin writer.
    pub exec_inputs: Mutex<HashMap<String, Pin<Box<dyn AsyncWrite + Send>>>>,
    /// Running log-stream tasks keyed by container id, so they can be cancelled.
    pub log_tasks: Mutex<HashMap<String, tokio::task::AbortHandle>>,
    /// Integrated-terminal sessions: session id -> shell stdin writer.
    pub term_inputs: Mutex<HashMap<String, Pin<Box<dyn AsyncWrite + Send>>>>,
    /// Integrated-terminal child processes, so they can be killed on close.
    pub term_children: Mutex<HashMap<String, tokio::process::Child>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            docker: Mutex::new(None),
            engine_child: Mutex::new(None),
            exec_inputs: Mutex::new(HashMap::new()),
            log_tasks: Mutex::new(HashMap::new()),
            term_inputs: Mutex::new(HashMap::new()),
            term_children: Mutex::new(HashMap::new()),
        }
    }

    /// Returns a connected bollard client, creating and caching it on first use.
    /// Constructing the client does not open a socket; connection failures
    /// surface on the first request instead.
    pub async fn docker(&self) -> crate::error::AppResult<Docker> {
        let mut guard = self.docker.lock().await;
        if guard.is_none() {
            *guard = Some(crate::docker::client::connect()?);
        }
        Ok(guard.as_ref().expect("docker client just set").clone())
    }

    /// Drops the cached client (e.g. after stopping the engine).
    pub async fn reset_docker(&self) {
        *self.docker.lock().await = None;
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
