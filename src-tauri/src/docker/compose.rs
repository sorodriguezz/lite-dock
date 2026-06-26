//! `docker compose` v2 driven through the CLI inside the engine distro.
//!
//! Compose has no stable Engine API, so the official path is the `docker compose`
//! plugin. We stream up/down/logs and parse `ls`/`ps --format json` for state.

use crate::error::{AppError, AppResult};
use crate::{config, wsl};
use tauri::AppHandle;

/// Common `wsl … docker compose -f <file> [-p <project>]` prefix.
fn base_args(file_win: &str, project: &str) -> Vec<String> {
    let file = wsl::win_to_wsl_path(file_win);
    let mut a: Vec<String> = vec![
        "-d".into(),
        config::DISTRO_NAME.into(),
        "-u".into(),
        "root".into(),
        "--".into(),
        "docker".into(),
        "compose".into(),
        "-f".into(),
        file,
    ];
    if !project.trim().is_empty() {
        a.push("-p".into());
        a.push(project.to_string());
    }
    a
}

/// Pre-create host directories for named volumes backed by a local bind mount
/// (`driver_opts: { type: none, o: bind, device: <path> }`).
///
/// Unlike a plain bind mount, the real Docker Engine refuses to auto-create the
/// host directory for these volumes, so Compose fails with
/// `failed to mount local volume … no such file or directory`. Docker Desktop
/// hides this by creating the directories for you; we replicate that here.
///
/// Best-effort: we resolve the project with `docker compose config --format
/// json` (same mechanism already used by `ls`/`ps`) and `mkdir -p` every bind
/// device inside the engine distro. Any failure is ignored so the subsequent
/// `up` still runs and surfaces the real error.
async fn ensure_bind_dirs(file_win: &str, project: &str) {
    let mut a = base_args(file_win, project);
    a.push("config".into());
    a.push("--format".into());
    a.push("json".into());
    let str_args: Vec<&str> = a.iter().map(|s| s.as_str()).collect();

    let (ok, out, _err) = match wsl::run_wsl(&str_args).await {
        Ok(t) => t,
        Err(_) => return,
    };
    if !ok {
        return;
    }
    let cfg: serde_json::Value = match serde_json::from_str(out.trim()) {
        Ok(v) => v,
        Err(_) => return,
    };

    // Collect bind `device` paths from top-level `volumes`.
    let mut dirs: Vec<String> = Vec::new();
    if let Some(vols) = cfg.get("volumes").and_then(|v| v.as_object()) {
        for def in vols.values() {
            let Some(opts) = def.get("driver_opts").and_then(|v| v.as_object()) else {
                continue;
            };
            let Some(device) = opts.get("device").and_then(|v| v.as_str()) else {
                continue;
            };
            let is_bind = opts
                .get("o")
                .and_then(|v| v.as_str())
                .map(|s| s.contains("bind"))
                .unwrap_or(false)
                || opts
                    .get("type")
                    .and_then(|v| v.as_str())
                    .map(|s| s == "none")
                    .unwrap_or(false);
            if !is_bind {
                continue;
            }
            // `device` is normally already a WSL path (e.g. `/mnt/c/...`); guard
            // against a Windows-style path slipping through.
            let path = wsl::win_to_wsl_path(device);
            if !dirs.contains(&path) {
                dirs.push(path);
            }
        }
    }
    if dirs.is_empty() {
        return;
    }

    let mut margs: Vec<&str> = vec![
        "-d",
        config::DISTRO_NAME,
        "-u",
        "root",
        "--",
        "mkdir",
        "-p",
    ];
    margs.extend(dirs.iter().map(|s| s.as_str()));
    let _ = wsl::run_wsl(&margs).await;
}

pub async fn up(app: &AppHandle, file_win: String, project: String) -> AppResult<i32> {
    // Create any missing host directories for local bind-mounted volumes before
    // bringing the project up, mirroring Docker Desktop's behaviour.
    ensure_bind_dirs(&file_win, &project).await;

    let mut a = base_args(&file_win, &project);
    a.push("up".into());
    a.push("-d".into());
    super::buildx::run_streaming(app, "compose-output", &a).await
}

pub async fn down(app: &AppHandle, file_win: String, project: String) -> AppResult<i32> {
    let mut a = base_args(&file_win, &project);
    a.push("down".into());
    super::buildx::run_streaming(app, "compose-output", &a).await
}

pub async fn logs(app: &AppHandle, file_win: String, project: String) -> AppResult<i32> {
    let mut a = base_args(&file_win, &project);
    a.push("logs".into());
    a.push("--no-color".into());
    a.push("--tail".into());
    a.push("200".into());
    super::buildx::run_streaming(app, "compose-output", &a).await
}

/// `docker compose ls --all --format json` → active/known projects.
pub async fn ls() -> AppResult<serde_json::Value> {
    let (ok, out, err) = wsl::run_wsl(&[
        "-d",
        config::DISTRO_NAME,
        "-u",
        "root",
        "--",
        "docker",
        "compose",
        "ls",
        "--all",
        "--format",
        "json",
    ])
    .await?;
    if !ok {
        return Err(AppError::other(format!("compose ls failed: {err}")));
    }
    let v: serde_json::Value =
        serde_json::from_str(out.trim()).unwrap_or_else(|_| serde_json::json!([]));
    Ok(v)
}

/// `docker compose ps --all --format json` for one project file.
pub async fn ps(file_win: String, project: String) -> AppResult<serde_json::Value> {
    let mut a = base_args(&file_win, &project);
    a.push("ps".into());
    a.push("--all".into());
    a.push("--format".into());
    a.push("json".into());
    let str_args: Vec<&str> = a.iter().map(|s| s.as_str()).collect();
    let (ok, out, err) = wsl::run_wsl(&str_args).await?;
    if !ok {
        return Err(AppError::other(format!("compose ps failed: {err}")));
    }
    // `compose ps` emits either a JSON array or NDJSON depending on version;
    // handle both.
    let trimmed = out.trim();
    if trimmed.starts_with('[') {
        return Ok(serde_json::from_str(trimmed).unwrap_or_else(|_| serde_json::json!([])));
    }
    let items: Vec<serde_json::Value> = trimmed
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    Ok(serde_json::Value::Array(items))
}
