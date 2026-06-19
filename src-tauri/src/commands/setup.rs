//! First-run / setup commands.

use crate::error::AppResult;
use crate::state::AppState;
use crate::wsl::{bootstrap, detect, lifecycle};
use tauri::{AppHandle, State};

/// Read-only probe used by the wizard to decide what to show.
#[tauri::command]
pub async fn setup_detect() -> AppResult<detect::WslStatus> {
    Ok(detect::detect().await)
}

/// Run the full first-run flow, emitting `setup-progress` events.
#[tauri::command]
pub async fn setup_run(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<bootstrap::SetupResult> {
    bootstrap::run(&app, state.inner()).await
}

/// Stop and unregister the engine distro (full reset; re-imported on next setup).
#[tauri::command]
pub async fn engine_reset(state: State<'_, AppState>) -> AppResult<()> {
    let _ = lifecycle::stop_engine(state.inner()).await;
    lifecycle::unregister().await
}
