//! Network commands.

use crate::docker::networks as net;
use crate::docker::types::NetworkDto;
use crate::error::AppResult;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_networks(state: State<'_, AppState>) -> AppResult<Vec<NetworkDto>> {
    let docker = state.docker().await?;
    net::list(&docker).await
}

#[tauri::command]
pub async fn create_network(
    state: State<'_, AppState>,
    name: String,
    driver: Option<String>,
) -> AppResult<()> {
    let docker = state.docker().await?;
    let driver = driver.unwrap_or_else(|| "bridge".to_string());
    net::create(&docker, &name, &driver).await
}

#[tauri::command]
pub async fn remove_network(state: State<'_, AppState>, name: String) -> AppResult<()> {
    let docker = state.docker().await?;
    net::remove(&docker, &name).await
}

#[tauri::command]
pub async fn prune_networks(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    let docker = state.docker().await?;
    net::prune(&docker).await
}

#[tauri::command]
pub async fn inspect_network(
    state: State<'_, AppState>,
    name: String,
) -> AppResult<serde_json::Value> {
    let docker = state.docker().await?;
    net::inspect(&docker, &name).await
}
