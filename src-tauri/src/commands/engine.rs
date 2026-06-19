//! Engine status / lifecycle commands.

use crate::config;
use crate::docker;
use crate::docker::types::EngineStatus;
use crate::error::AppResult;
use crate::state::AppState;
use crate::wsl::lifecycle;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn engine_status(state: State<'_, AppState>) -> AppResult<EngineStatus> {
    let docker = state.docker().await?;
    Ok(docker::system::status(&docker).await)
}

#[tauri::command]
pub async fn engine_start(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    lifecycle::ensure_running(&app, state.inner()).await
}

#[tauri::command]
pub async fn engine_stop(state: State<'_, AppState>) -> AppResult<()> {
    lifecycle::stop_engine(state.inner()).await
}

#[tauri::command]
pub async fn engine_restart(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    lifecycle::stop_engine(state.inner()).await?;
    lifecycle::ensure_running(&app, state.inner()).await
}

/// Last lines of the in-distro dockerd log (for diagnostics).
#[tauri::command]
pub async fn engine_logs() -> AppResult<String> {
    lifecycle::engine_log_tail(200).await
}

/// Persist DOCKER_HOST so the Windows `docker` CLI talks to LiteDock's engine.
#[tauri::command]
pub fn enable_docker_cli() -> AppResult<String> {
    crate::wsl::set_docker_host();
    Ok(config::engine_tcp_url())
}

/// Remove DOCKER_HOST so the Windows `docker` CLI stops using LiteDock.
#[tauri::command]
pub fn disable_docker_cli() -> AppResult<()> {
    crate::wsl::unset_docker_host();
    Ok(())
}

/// Whether the Windows `docker` CLI is currently wired to LiteDock's engine.
#[tauri::command]
pub fn cli_status() -> AppResult<bool> {
    Ok(crate::wsl::docker_host_enabled())
}

/// Open a Windows path (e.g. a `\\wsl$` UNC to a volume) in File Explorer.
#[tauri::command]
pub fn open_path(path: String) -> AppResult<()> {
    std::process::Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map_err(|e| crate::error::AppError::other(format!("no se pudo abrir el explorador: {e}")))?;
    Ok(())
}

/// Open an external https URL in the user's default browser. Restricted to
/// https so only app-generated links (e.g. Docker Hub) can be launched.
#[tauri::command]
pub fn open_url(url: String) -> AppResult<()> {
    if !url.starts_with("https://") {
        return Err(crate::error::AppError::other("URL no permitida".to_string()));
    }
    std::process::Command::new("explorer.exe")
        .arg(&url)
        .spawn()
        .map_err(|e| crate::error::AppError::other(format!("no se pudo abrir el navegador: {e}")))?;
    Ok(())
}

/// Upgrade the engine packages (dockerd, buildkit, compose…) to the latest in
/// the distro's Alpine branch, then reload the daemon. Streams to `engine-update`.
#[tauri::command]
pub async fn engine_update(app: AppHandle, state: State<'_, AppState>) -> AppResult<i32> {
    lifecycle::ensure_running(&app, state.inner()).await?;
    let args: Vec<String> = vec![
        "-d".into(),
        config::DISTRO_NAME.into(),
        "-u".into(),
        "root".into(),
        "--".into(),
        "sh".into(),
        "-lc".into(),
        "apk update && apk upgrade && echo '----' && docker --version".into(),
    ];
    let code = crate::docker::buildx::run_streaming(&app, "engine-update", &args).await?;
    // Reload dockerd so the upgraded binaries take effect.
    let _ = lifecycle::stop_engine(state.inner()).await;
    lifecycle::ensure_running(&app, state.inner()).await?;
    Ok(code)
}

#[tauri::command]
pub async fn system_df(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    let docker = state.docker().await?;
    docker::system::df(&docker).await
}

#[derive(serde::Serialize)]
pub struct AppUsage {
    pub cpu_percent: f64,
    pub ram_bytes: u64,
}

/// LiteDock's own resource footprint (app + WebView2 + the WSL engine VM).
#[tauri::command]
pub async fn app_usage() -> AppResult<AppUsage> {
    let (cpu_percent, ram_bytes) = crate::wsl::app_usage().await;
    Ok(AppUsage {
        cpu_percent,
        ram_bytes,
    })
}

#[derive(serde::Serialize)]
pub struct WslConfig {
    /// WSL2 RAM cap in MB, if one is configured.
    pub memory_mb: Option<u32>,
    pub auto_reclaim: bool,
}

/// Read the current WSL2 memory settings from `.wslconfig`.
#[tauri::command]
pub fn wsl_config_get() -> AppResult<WslConfig> {
    let (memory_mb, auto_reclaim) = crate::wsl::read_wsl_config();
    Ok(WslConfig {
        memory_mb,
        auto_reclaim,
    })
}

/// Write the WSL2 memory settings, fully restart WSL so they apply, and bring
/// the engine back up. `memory_mb = None` removes the cap.
#[tauri::command]
pub async fn wsl_config_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    memory_mb: Option<u32>,
    auto_reclaim: bool,
) -> AppResult<()> {
    crate::wsl::write_wsl_config(memory_mb, auto_reclaim)?;
    // Stop our engine cleanly, full WSL shutdown (re-reads .wslconfig), restart.
    let _ = lifecycle::stop_engine(state.inner()).await;
    crate::wsl::shutdown_all().await;
    lifecycle::ensure_running(&app, state.inner()).await
}
