//! Container commands, including streaming logs, stats and interactive exec.

use crate::docker::containers as c;
use crate::docker::types::{ContainerDto, StatsDto};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
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
    let results = c::exec_start(&docker, &id, command).await?;
    let session = new_session_id();

    match results {
        StartExecResults::Attached { mut output, input } => {
            state
                .exec_inputs
                .lock()
                .await
                .insert(session.clone(), input);

            let app2 = app.clone();
            let sess = session.clone();
            tokio::spawn(async move {
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
                let _ = app2.emit("exec-exit", sess.clone());
            });
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

/// Close an exec session (drops stdin → the shell receives EOF and exits).
#[tauri::command]
pub async fn exec_kill(state: State<'_, AppState>, session: String) -> AppResult<()> {
    state.exec_inputs.lock().await.remove(&session);
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
    )
    .await
}
