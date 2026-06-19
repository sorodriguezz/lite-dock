//! `docker build` via BuildKit, run as the CLI inside the engine distro with
//! output streamed to the frontend.
//!
//! Why the CLI and not bollard? BuildKit needs the buildx gRPC session that the
//! `docker buildx` client sets up; reproducing it over the raw Engine API is far
//! more complex than streaming the CLI's `--progress=plain` output, which is
//! exactly what the UI wants to display anyway. See docs/ARCHITECTURE.md.

use crate::error::{AppError, AppResult};
use crate::{config, wsl};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};

/// A single line of streamed CLI output.
#[derive(serde::Serialize, Clone)]
pub struct OutputLine {
    pub line: String,
    /// "stdout" | "stderr" | "status"
    pub stream: String,
}

/// Run `docker buildx build` inside the engine distro, streaming output to the
/// `build-output` event. `context_win` / `dockerfile_win` are Windows paths.
pub async fn build(
    app: &AppHandle,
    context_win: String,
    dockerfile_win: String,
    tag: String,
) -> AppResult<i32> {
    let context = wsl::win_to_wsl_path(&context_win);
    let dockerfile = wsl::win_to_wsl_path(&dockerfile_win);

    let mut args: Vec<String> = vec![
        "-d".into(),
        config::DISTRO_NAME.into(),
        "-u".into(),
        "root".into(),
        "--".into(),
        "docker".into(),
        "buildx".into(),
        "build".into(),
        "--progress=plain".into(),
        "-f".into(),
        dockerfile,
    ];
    if !tag.trim().is_empty() {
        args.push("-t".into());
        args.push(tag);
    }
    args.push(context);

    run_streaming(app, "build-output", &args).await
}

/// Spawn `wsl.exe <args>` and stream stdout+stderr lines to `event`.
/// Emits a final `__EXIT__ <code>` status line. Returns the exit code.
pub async fn run_streaming(app: &AppHandle, event: &str, args: &[String]) -> AppResult<i32> {
    use std::process::Stdio;

    let mut child = wsl::command("wsl.exe")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::other(format!("failed to spawn wsl.exe: {e}")))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let app_out = app.clone();
    let ev_out = event.to_string();
    let out_task = tokio::spawn(async move {
        if let Some(out) = stdout {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app_out.emit(
                    &ev_out,
                    OutputLine {
                        line,
                        stream: "stdout".into(),
                    },
                );
            }
        }
    });

    let app_err = app.clone();
    let ev_err = event.to_string();
    let err_task = tokio::spawn(async move {
        if let Some(err) = stderr {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                // BuildKit writes its progress to stderr; surface it as output.
                let _ = app_err.emit(
                    &ev_err,
                    OutputLine {
                        line,
                        stream: "stderr".into(),
                    },
                );
            }
        }
    });

    let status = child
        .wait()
        .await
        .map_err(|e| AppError::other(format!("process error: {e}")))?;
    let _ = out_task.await;
    let _ = err_task.await;

    let code = status.code().unwrap_or(-1);
    let _ = app.emit(
        event,
        OutputLine {
            line: format!("__EXIT__ {code}"),
            stream: "status".into(),
        },
    );
    Ok(code)
}
