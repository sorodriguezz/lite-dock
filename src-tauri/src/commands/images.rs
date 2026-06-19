//! Image commands.

use crate::docker::images as img;
use crate::docker::types::ImageDto;
use crate::error::AppResult;
use crate::state::AppState;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn list_images(state: State<'_, AppState>) -> AppResult<Vec<ImageDto>> {
    let docker = state.docker().await?;
    img::list(&docker).await
}

/// Pull `image[:tag]`, streaming per-layer progress on the `image-pull` event.
#[tauri::command]
pub async fn pull_image(
    app: AppHandle,
    state: State<'_, AppState>,
    image: String,
    tag: Option<String>,
) -> AppResult<()> {
    use futures_util::StreamExt;
    let docker = state.docker().await?;
    let tag = tag.unwrap_or_else(|| "latest".to_string());
    let stream = img::pull_stream(&docker, &image, &tag);
    futures_util::pin_mut!(stream);
    while let Some(item) = stream.next().await {
        match item {
            Ok(info) => {
                let _ = app.emit("image-pull", info);
            }
            Err(e) => {
                let _ = app.emit("image-pull-error", e.to_string());
                return Err(e.into());
            }
        }
    }
    let _ = app.emit("image-pull-done", image);
    Ok(())
}

#[tauri::command]
pub async fn remove_image(state: State<'_, AppState>, id: String, force: bool) -> AppResult<()> {
    let docker = state.docker().await?;
    img::remove(&docker, &id, force).await
}

#[tauri::command]
pub async fn prune_images(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    let docker = state.docker().await?;
    img::prune_dangling(&docker).await
}

#[tauri::command]
pub async fn image_history(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<serde_json::Value> {
    let docker = state.docker().await?;
    img::history(&docker, &id).await
}

#[tauri::command]
pub async fn inspect_image(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<serde_json::Value> {
    let docker = state.docker().await?;
    img::inspect(&docker, &id).await
}

#[tauri::command]
pub async fn search_images(
    state: State<'_, AppState>,
    term: String,
    limit: Option<i64>,
) -> AppResult<Vec<crate::docker::types::SearchResultDto>> {
    let docker = state.docker().await?;
    img::search(&docker, &term, limit.unwrap_or(25)).await
}
