//! Container commands, including streaming logs, stats and interactive exec.

use crate::docker::containers as c;
use crate::docker::types::{ContainerDto, StatsDto};
use crate::error::{AppError, AppResult};
use crate::state::{AppState, ExecMeta};
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub async fn list_containers(state: State<'_, AppState>) -> AppResult<Vec<ContainerDto>> {
    let docker = state.docker().await?;
    c::list(&docker).await
}

#[tauri::command]
pub async fn start_container(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let docker = state.docker().await?;
    c::start(&docker, &id).await
}

#[tauri::command]
pub async fn stop_container(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let docker = state.docker().await?;
    c::stop(&docker, &id).await
}

#[tauri::command]
pub async fn restart_container(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let docker = state.docker().await?;
    c::restart(&docker, &id).await
}

#[tauri::command]
pub async fn pause_container(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let docker = state.docker().await?;
    c::pause(&docker, &id).await
}

#[tauri::command]
pub async fn unpause_container(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let docker = state.docker().await?;
    c::unpause(&docker, &id).await
}

#[tauri::command]
pub async fn kill_container(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let docker = state.docker().await?;
    c::kill(&docker, &id).await
}

#[tauri::command]
pub async fn remove_container(state: State<'_, AppState>, id: String, force: bool) -> AppResult<()> {
    let docker = state.docker().await?;
    c::remove(&docker, &id, force).await
}

#[tauri::command]
pub async fn inspect_container(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<serde_json::Value> {
    let docker = state.docker().await?;
    c::inspect(&docker, &id).await
}

#[tauri::command]
pub async fn container_stats(state: State<'_, AppState>, id: String) -> AppResult<StatsDto> {
    let docker = state.docker().await?;
    c::stats_once(&docker, &id).await
}

// ---------------------------------------------------------------------------
// Streaming logs
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone)]
struct LogLine {
    id: String,
    line: String,
}

/// Start streaming logs for `id`. Lines are emitted on the `container-log`
/// event. Cancels any previous stream for the same container.
#[tauri::command]
pub async fn container_logs_start(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    tail: Option<String>,
) -> AppResult<()> {
    use futures_util::StreamExt;
    let docker = state.docker().await?;
    let tail = tail.unwrap_or_else(|| "200".to_string());
    stop_log_task(state.inner(), &id).await;

    let id_task = id.clone();
    let app2 = app.clone();
    let handle = tokio::spawn(async move {
        let stream = c::logs_stream(&docker, &id_task, true, &tail);
        futures_util::pin_mut!(stream);
        while let Some(item) = stream.next().await {
            match item {
                Ok(output) => {
                    let _ = app2.emit(
                        "container-log",
                        LogLine {
                            id: id_task.clone(),
                            line: log_output_to_string(output),
                        },
                    );
                }
                Err(_) => break,
            }
        }
    });
    state
        .log_tasks
        .lock()
        .await
        .insert(id, handle.abort_handle());
    Ok(())
}

#[tauri::command]
pub async fn container_logs_stop(state: State<'_, AppState>, id: String) -> AppResult<()> {
    stop_log_task(state.inner(), &id).await;
    Ok(())
}

async fn stop_log_task(state: &AppState, id: &str) {
    if let Some(h) = state.log_tasks.lock().await.remove(id) {
        h.abort();
    }
}

fn log_output_to_string(o: bollard::container::LogOutput) -> String {
    let bytes = match o {
        bollard::container::LogOutput::StdOut { message }
        | bollard::container::LogOutput::StdErr { message }
        | bollard::container::LogOutput::Console { message }
        | bollard::container::LogOutput::StdIn { message } => message,
    };
    String::from_utf8_lossy(&bytes).to_string()
}

// ---------------------------------------------------------------------------
// Interactive exec
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone)]
struct ExecLine {
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
    format!("exec-{t}-{n}")
}

/// Open an interactive exec session (default `/bin/sh`). Output is streamed on
/// the `exec-output` event; session end fires `exec-exit`. Returns the session id.
#[tauri::command]
pub async fn exec_start(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    cmd: Option<Vec<String>>,
) -> AppResult<String> {
    use bollard::exec::StartExecResults;
    use futures_util::StreamExt;

    let docker = state.docker().await?;
    let command = cmd.unwrap_or_else(|| vec!["/bin/sh".to_string()]);
    let (exec_id, results) = c::exec_start(&docker, &id, command).await?;
    let session = new_session_id();

    match results {
        StartExecResults::Attached { mut output, input } => {
            state
                .exec_inputs
                .lock()
                .await
                .insert(session.clone(), input);

            // Hold the meta lock until the reader is registered, so its
            // end-of-session cleanup can't run before the insert below.
            let mut meta = state.exec_meta.lock().await;
            let app2 = app.clone();
            let sess = session.clone();
            let reader = tokio::spawn(async move {
                while let Some(item) = output.next().await {
                    match item {
                        Ok(out) => {
                            let _ = app2.emit(
                                "exec-output",
                                ExecLine {
                                    session: sess.clone(),
                                    line: log_output_to_string(out),
                                },
                            );
                        }
                        Err(_) => break,
                    }
                }
                // Session ended: clean up the stored stdin writer.
                let st = app2.state::<AppState>();
                st.exec_inputs.lock().await.remove(&sess);
                st.exec_meta.lock().await.remove(&sess);
                let _ = app2.emit("exec-exit", sess.clone());
            });
            meta.insert(
                session.clone(),
                ExecMeta {
                    exec_id,
                    reader: reader.abort_handle(),
                },
            );
            Ok(session)
        }
        StartExecResults::Detached => Err(AppError::other("exec se inició en modo detached")),
    }
}

/// Write `data` (typically keystrokes) to an exec session's stdin.
#[tauri::command]
pub async fn exec_write(
    state: State<'_, AppState>,
    session: String,
    data: String,
) -> AppResult<()> {
    use tokio::io::AsyncWriteExt;
    let mut guard = state.exec_inputs.lock().await;
    match guard.get_mut(&session) {
        Some(input) => {
            input.write_all(data.as_bytes()).await?;
            input.flush().await?;
            Ok(())
        }
        None => Err(AppError::other("sesión de exec no encontrada")),
    }
}

/// Close an exec session and really stop what runs in it. Just dropping stdin
/// is not enough: with a TTY the process never sees EOF, so `top`, `vim`,
/// `tail -f` (even the idle shell) kept running and emitting `exec-output`.
/// Returns at once; the cleanup runs in the background:
/// 1. Ctrl-C (foreground job) + Ctrl-D (EOF for the shell prompt) on stdin —
///    the cheap, graceful path; Ctrl-D is harmless if a full-screen app eats it
///    (unlike typing `exit` into e.g. vim).
/// 2. If the exec is still running after a moment: hang up its session / kill
///    it from the engine distro (`c::exec_terminate`).
/// 3. Stop forwarding its output.
#[tauri::command]
pub async fn exec_kill(
    app: AppHandle,
    state: State<'_, AppState>,
    session: String,
) -> AppResult<()> {
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;

    let input = state.exec_inputs.lock().await.remove(&session);
    let meta = state.exec_meta.lock().await.remove(&session);
    let docker = state.docker().await.ok();

    tokio::spawn(async move {
        if let Some(mut input) = input {
            let _ = tokio::time::timeout(Duration::from_secs(1), async {
                input.write_all(b"\x03").await?;
                input.flush().await?;
                // Let the shell take the terminal back before the EOF.
                tokio::time::sleep(Duration::from_millis(150)).await;
                input.write_all(b"\x04").await?;
                input.flush().await
            })
            .await;
        }
        let Some(meta) = meta else {
            return; // already finished (or unknown session)
        };
        if let Some(docker) = docker {
            // Give the graceful path ~0.5 s before escalating.
            let mut running = true;
            for _ in 0..5 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                running = docker
                    .inspect_exec(&meta.exec_id)
                    .await
                    .map(|i| i.running == Some(true))
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
            if running {
                c::exec_terminate(&docker, &meta.exec_id).await;
            }
        }
        // Whatever happened, the UI is gone: stop emitting its output. If the
        // reader was still alive, its own `exec-exit` won't fire — send it here.
        if !meta.reader.is_finished() {
            meta.reader.abort();
            let _ = app.emit("exec-exit", session);
        }
    });
    Ok(())
}

/// Apply memory (MB) / CPU (cores) limits to a container. `memory_mb = 0` clears.
#[tauri::command]
pub async fn container_update_limits(
    state: State<'_, AppState>,
    id: String,
    memory_mb: Option<i64>,
    cpus: Option<f64>,
) -> AppResult<()> {
    let docker = state.docker().await?;
    c::update_resources(&docker, &id, memory_mb, cpus).await
}

/// List a directory inside the container (file browser).
#[tauri::command]
pub async fn container_browse(
    state: State<'_, AppState>,
    id: String,
    path: Option<String>,
) -> AppResult<Vec<crate::docker::types::FileEntryDto>> {
    let docker = state.docker().await?;
    c::browse(&docker, &id, &path.unwrap_or_else(|| "/".to_string())).await
}

/// Delete a file/dir inside the container (file browser).
#[tauri::command]
pub async fn container_delete_path(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> AppResult<()> {
    let docker = state.docker().await?;
    c::delete_path(&docker, &id, &path).await
}

/// Upload a host file into a directory inside the container (file browser).
#[tauri::command]
pub async fn container_upload(
    state: State<'_, AppState>,
    id: String,
    dest_dir: String,
    host_path: String,
) -> AppResult<()> {
    let docker = state.docker().await?;
    c::upload_file(&docker, &id, &dest_dir, &host_path).await
}

/// Read a file/dir from the container, returned base64-encoded (dir → tar).
#[tauri::command]
pub async fn container_download(
    state: State<'_, AppState>,
    id: String,
    path: String,
    is_dir: bool,
) -> AppResult<String> {
    let docker = state.docker().await?;
    c::download(&docker, &id, &path, is_dir).await
}

/// Download ONE regular file from the container straight to the Windows path
/// `dest` (created/overwritten) — no base64 / `number[]` round trip through the
/// webview, so it scales to big files. Returns the number of bytes written.
#[tauri::command]
pub async fn container_download_to_host(
    state: State<'_, AppState>,
    id: String,
    path: String,
    dest: String,
) -> AppResult<u64> {
    let docker = state.docker().await?;
    c::download_to_host(&docker, &id, &path, &dest).await
}

/// Write raw bytes to a host path (used to save the files/zips the user downloads).
#[tauri::command]
pub async fn write_host_file(path: String, data: Vec<u8>) -> AppResult<()> {
    std::fs::write(&path, &data)
        .map_err(|e| AppError::other(format!("no se pudo guardar el archivo: {e}")))?;
    Ok(())
}

/// Create + start a container from an image (Portainer-style run form).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn run_container(
    state: State<'_, AppState>,
    image: String,
    name: Option<String>,
    ports: Vec<String>,
    env: Vec<String>,
    volumes: Vec<String>,
    restart: String,
    pull: bool,
    publish_all: bool,
    auto_remove: Option<bool>,
) -> AppResult<String> {
    let docker = state.docker().await?;
    c::create_and_start(
        &docker,
        &image,
        name.as_deref(),
        &ports,
        &env,
        &volumes,
        &restart,
        pull,
        publish_all,
        auto_remove.unwrap_or(false),
    )
    .await
}
