//! Integrated terminal: a persistent host (PowerShell) or engine (WSL) shell,
//! streamed to the frontend. It is line-based over plain pipes — no PTY — to
//! stay dependency-free and lightweight; the UI provides the input box. A real
//! ConPTY (e.g. via `portable-pty`) would add prompts/colors but more weight.

use crate::config;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(serde::Serialize, Clone)]
struct TermLine {
    session: String,
    line: String,
}

fn new_session_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("term-{t}-{n}")
}

/// Build the shell command for the requested kind:
/// - `host`:   Windows PowerShell, reading commands from its piped stdin.
/// - `engine`: an interactive shell inside the litedock-engine WSL distro.
///
/// Uses `wsl::command` so the spawned process never flashes a console window.
fn build_command(kind: &str) -> AppResult<tokio::process::Command> {
    let mut cmd = match kind {
        "host" => {
            let mut c = crate::wsl::command("powershell.exe");
            // Pipes default to the OEM code page (e.g. CP850); switch both
            // directions to UTF-8 so accented text round-trips with the UI.
            c.args([
                "-NoLogo",
                "-NoProfile",
                "-NoExit",
                "-Command",
                "[Console]::InputEncoding=[Console]::OutputEncoding=[Text.Encoding]::UTF8",
            ]);
            c
        }
        "engine" => {
            let mut c = crate::wsl::command("wsl.exe");
            c.args(["-d", config::DISTRO_NAME, "-u", "root", "--", "sh", "-i"]);
            c
        }
        other => return Err(AppError::other(format!("shell desconocido: {other}"))),
    };
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    Ok(cmd)
}

/// Start an integrated terminal session. Output streams on `terminal-output`;
/// the session end fires `terminal-exit`. Returns the session id.
#[tauri::command]
pub async fn terminal_start(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: String,
) -> AppResult<String> {
    use crate::wsl::read_line_lossy;
    use tokio::io::BufReader;

    let mut cmd = build_command(&kind)?;
    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::other(format!("no se pudo iniciar la terminal: {e}")))?;

    let session = new_session_id();

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::other("la terminal no expuso stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::other("la terminal no expuso stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::other("la terminal no expuso stderr"))?;

    state
        .term_inputs
        .lock()
        .await
        .insert(session.clone(), Box::pin(stdin));

    // stderr reader (shell prompts / error output).
    {
        let app2 = app.clone();
        let sess = session.clone();
        tokio::spawn(async move {
            let (mut rd, mut buf) = (BufReader::new(stderr), Vec::new());
            while let Some(line) = read_line_lossy(&mut rd, &mut buf).await {
                let _ = app2.emit("terminal-output", TermLine { session: sess.clone(), line });
            }
        });
    }

    // stdout reader; when it ends the shell has exited → clean up + notify.
    {
        let app2 = app.clone();
        let sess = session.clone();
        tokio::spawn(async move {
            let (mut rd, mut buf) = (BufReader::new(stdout), Vec::new());
            while let Some(line) = read_line_lossy(&mut rd, &mut buf).await {
                let _ = app2.emit("terminal-output", TermLine { session: sess.clone(), line });
            }
            let st = app2.state::<AppState>();
            st.term_inputs.lock().await.remove(&sess);
            st.term_children.lock().await.remove(&sess);
            let _ = app2.emit("terminal-exit", sess.clone());
        });
    }

    state
        .term_children
        .lock()
        .await
        .insert(session.clone(), child);

    Ok(session)
}

/// Write a line (command + newline) to a terminal session's stdin.
#[tauri::command]
pub async fn terminal_write(
    state: State<'_, AppState>,
    session: String,
    data: String,
) -> AppResult<()> {
    use tokio::io::AsyncWriteExt;
    let mut guard = state.term_inputs.lock().await;
    match guard.get_mut(&session) {
        Some(input) => {
            input.write_all(data.as_bytes()).await?;
            input.flush().await?;
            Ok(())
        }
        None => Err(AppError::other("sesión de terminal no encontrada")),
    }
}

/// Close a terminal session: drop its stdin and kill the child process.
#[tauri::command]
pub async fn terminal_kill(state: State<'_, AppState>, session: String) -> AppResult<()> {
    state.term_inputs.lock().await.remove(&session);
    if let Some(mut child) = state.term_children.lock().await.remove(&session) {
        let _ = child.kill().await;
    }
    Ok(())
}
