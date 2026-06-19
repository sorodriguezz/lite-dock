//! Docker Compose commands (CLI v2, streamed).

use crate::docker::compose;
use crate::error::AppResult;
use crate::state::AppState;
use crate::wsl::lifecycle;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn compose_up(
    app: AppHandle,
    state: State<'_, AppState>,
    file: String,
    project: String,
) -> AppResult<i32> {
    lifecycle::ensure_running(&app, state.inner()).await?;
    compose::up(&app, file, project).await
}

#[tauri::command]
pub async fn compose_down(
    app: AppHandle,
    state: State<'_, AppState>,
    file: String,
    project: String,
) -> AppResult<i32> {
    lifecycle::ensure_running(&app, state.inner()).await?;
    compose::down(&app, file, project).await
}

#[tauri::command]
pub async fn compose_logs(
    app: AppHandle,
    state: State<'_, AppState>,
    file: String,
    project: String,
) -> AppResult<i32> {
    lifecycle::ensure_running(&app, state.inner()).await?;
    compose::logs(&app, file, project).await
}

#[tauri::command]
pub async fn compose_ls(app: AppHandle, state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    lifecycle::ensure_running(&app, state.inner()).await?;
    compose::ls().await
}

#[tauri::command]
pub async fn compose_ps(
    app: AppHandle,
    state: State<'_, AppState>,
    file: String,
    project: String,
) -> AppResult<serde_json::Value> {
    lifecycle::ensure_running(&app, state.inner()).await?;
    compose::ps(file, project).await
}
