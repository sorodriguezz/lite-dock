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

pub async fn up(app: &AppHandle, file_win: String, project: String) -> AppResult<i32> {
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
