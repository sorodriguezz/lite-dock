//! Engine distro lifecycle: import, start dockerd, wait, stop/terminate.

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::{config, docker, wsl};
use std::path::Path;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Import the bundled rootfs as the `litedock-engine` distro. Idempotent at the
/// call site (check `detect().distro_imported` first).
pub async fn import_distro(rootfs_tar: &Path, data_dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(data_dir)?;
    let tar = rootfs_tar.to_string_lossy().to_string();
    let dir = data_dir.to_string_lossy().to_string();
    let (ok, _out, err) = wsl::run_wsl(&[
        "--import",
        config::DISTRO_NAME,
        &dir,
        &tar,
        "--version",
        "2",
    ])
    .await?;
    if !ok {
        return Err(AppError::setup(format!("`wsl --import` falló: {err}")));
    }
    Ok(())
}

/// Unregister the distro (uninstall / reset). Destroys its data.
pub async fn unregister() -> AppResult<()> {
    let (ok, _o, err) = wsl::run_wsl(&["--unregister", config::DISTRO_NAME]).await?;
    if !ok {
        return Err(AppError::setup(format!("`wsl --unregister` falló: {err}")));
    }
    Ok(())
}

/// Terminate the distro to free all of its RAM.
pub async fn terminate() -> AppResult<()> {
    let _ = wsl::run_wsl(&["--terminate", config::DISTRO_NAME]).await?;
    Ok(())
}

/// Spawn dockerd (foreground) inside the distro, streaming its stdout+stderr to
/// the frontend as `engine-log` events, and keep the child handle in state.
/// No-op if a handle already exists.
pub async fn start_engine(app: &AppHandle, state: &AppState) -> AppResult<()> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut guard = state.engine_child.lock().await;
    if guard.is_some() {
        return Ok(());
    }
    let mut child = wsl::command("wsl.exe")
        .args([
            "-d",
            config::DISTRO_NAME,
            "-u",
            "root",
            "--exec",
            config::INIT_SCRIPT,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::setup(format!("no se pudo iniciar el motor: {e}")))?;

    // Stream both pipes to the UI so failures (bad interpreter, overlay2,
    // iptables…) are visible instead of a silent timeout.
    if let Some(out) = child.stdout.take() {
        let app2 = app.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app2.emit("engine-log", line);
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let app2 = app.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app2.emit("engine-log", line);
            }
        });
    }

    *guard = Some(child);
    Ok(())
}

/// Read the tail of the in-distro dockerd log (best effort).
pub async fn engine_log_tail(lines: usize) -> AppResult<String> {
    let cmd = format!("tail -n {lines} /var/log/litedock-dockerd.log 2>/dev/null || true");
    let (_ok, out, _e) = wsl::run_wsl(&[
        "-d",
        config::DISTRO_NAME,
        "-u",
        "root",
        "--",
        "sh",
        "-lc",
        cmd.as_str(),
    ])
    .await?;
    Ok(out)
}

/// Wait until dockerd answers, up to `timeout_secs`. On timeout, includes the
/// tail of the engine log so the failure is visible.
pub async fn wait_until_ready(timeout_secs: u64) -> AppResult<()> {
    let docker = docker::client::connect()?;
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        if docker.version().await.is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let tail = engine_log_tail(40).await.unwrap_or_default();
            let msg = if tail.trim().is_empty() {
                "el motor no respondió a tiempo (sin logs de dockerd; es posible que no llegara a arrancar)".to_string()
            } else {
                format!("el motor no respondió a tiempo. Últimas líneas de dockerd:\n{tail}")
            };
            return Err(AppError::setup(msg));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Ensure the engine is up: start it (if needed) and wait for readiness.
pub async fn ensure_running(app: &AppHandle, state: &AppState) -> AppResult<()> {
    // Fast path: already answering.
    if let Ok(docker) = docker::client::connect() {
        if docker.version().await.is_ok() {
            return Ok(());
        }
    }
    start_engine(app, state).await?;
    wait_until_ready(45).await?;
    Ok(())
}

/// Stop the engine and terminate the distro so idle RAM drops to ~0.
pub async fn stop_engine(state: &AppState) -> AppResult<()> {
    let _ = terminate().await; // kills dockerd + everything in the distro
    let mut guard = state.engine_child.lock().await;
    if let Some(mut child) = guard.take() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
    state.reset_docker().await;
    Ok(())
}
