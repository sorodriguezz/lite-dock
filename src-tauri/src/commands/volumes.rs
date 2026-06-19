//! Volume commands.

use crate::docker::types::VolumeDto;
use crate::docker::volumes as vol;
use crate::error::AppResult;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_volumes(state: State<'_, AppState>) -> AppResult<Vec<VolumeDto>> {
    let docker = state.docker().await?;
    vol::list(&docker).await
}

#[tauri::command]
pub async fn create_volume(
    state: State<'_, AppState>,
    name: String,
    driver: Option<String>,
) -> AppResult<()> {
    let docker = state.docker().await?;
    let driver = driver.unwrap_or_else(|| "local".to_string());
    vol::create(&docker, &name, &driver).await
}

#[tauri::command]
pub async fn remove_volume(state: State<'_, AppState>, name: String, force: bool) -> AppResult<()> {
    let docker = state.docker().await?;
    vol::remove(&docker, &name, force).await
}

#[tauri::command]
pub async fn prune_volumes(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    let docker = state.docker().await?;
    vol::prune(&docker).await
}

#[tauri::command]
pub async fn inspect_volume(
    state: State<'_, AppState>,
    name: String,
) -> AppResult<serde_json::Value> {
    let docker = state.docker().await?;
    vol::inspect(&docker, &name).await
}
