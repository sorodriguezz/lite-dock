//! Dockerfile build command (BuildKit via CLI, streamed).

use crate::error::AppResult;
use crate::state::AppState;
use crate::wsl::lifecycle;
use tauri::{AppHandle, State};

/// Build an image from a Dockerfile + context. Output streams on `build-output`.
/// `context` and `dockerfile` are Windows paths; they are translated to the
/// distro's drvfs mounts. Returns the process exit code (0 == success).
#[tauri::command]
pub async fn build_image(
    app: AppHandle,
    state: State<'_, AppState>,
    context: String,
    dockerfile: String,
    tag: String,
) -> AppResult<i32> {
    lifecycle::ensure_running(&app, state.inner()).await?;
    crate::docker::buildx::build(&app, context, dockerfile, tag).await
}
