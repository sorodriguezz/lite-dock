//! First-run orchestration. Detects what's already present and only does the
//! missing steps — this is the "no reinstalar WSL/distro" behaviour.

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::wsl::{detect, lifecycle};
use crate::{config, wsl};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Clone)]
pub struct SetupProgress {
    pub step: String,
    /// "running" | "ok" | "error" | "info"
    pub status: String,
    pub message: String,
}

#[derive(Serialize, Clone)]
pub struct SetupResult {
    pub ok: bool,
    pub needs_reboot: bool,
    pub message: String,
}

fn emit(app: &AppHandle, step: &str, status: &str, message: &str) {
    let _ = app.emit(
        "setup-progress",
        SetupProgress {
            step: step.to_string(),
            status: status.to_string(),
            message: message.to_string(),
        },
    );
}

/// Resolve the bundled rootfs tar from the Tauri resource directory.
fn resource_tar(app: &AppHandle) -> AppResult<std::path::PathBuf> {
    let dir = app.path().resource_dir().map_err(|e| {
        AppError::setup(format!("no se pudo resolver el directorio de recursos: {e}"))
    })?;
    let tar = dir.join(config::ROOTFS_RESOURCE_REL);
    if !tar.exists() {
        return Err(AppError::setup(format!(
            "no se encontró el motor empaquetado en {}",
            tar.display()
        )));
    }
    Ok(tar)
}

/// Full first-run / ensure-ready flow. Idempotent.
pub async fn run(app: &AppHandle, state: &AppState) -> AppResult<SetupResult> {
    // Step 1 — detect
    emit(app, "detect", "running", "Comprobando WSL2…");
    let status = detect::detect().await;
    emit(app, "detect", "ok", &status.message);

    // Step 2 — ensure WSL2 (skipped entirely when already present)
    if !status.wsl_present || !status.wsl2_ready {
        emit(
            app,
            "wsl",
            "running",
            "Instalando WSL2 (puede pedir permisos de administrador)…",
        );
        match install_wsl().await {
            Ok(needs_reboot) => {
                if needs_reboot {
                    emit(app, "wsl", "info", "WSL2 instalado. Reinicia Windows para continuar.");
                    return Ok(SetupResult {
                        ok: false,
                        needs_reboot: true,
                        message: "Reinicia Windows y vuelve a abrir LiteDock para terminar."
                            .into(),
                    });
                }
                emit(app, "wsl", "ok", "WSL2 listo.");
            }
            Err(e) => {
                emit(app, "wsl", "error", &e.to_string());
                return Err(e);
            }
        }
    } else {
        emit(app, "wsl", "ok", "WSL2 ya está instalado. Se omite.");
    }

    // Step 3 — import the engine distro (only if missing)
    let status = detect::detect().await;
    if !status.distro_imported {
        emit(app, "import", "running", "Importando el motor de LiteDock…");
        let tar = resource_tar(app)?;
        let data_dir = config::engine_data_dir();
        lifecycle::import_distro(&tar, &data_dir).await?;
        emit(app, "import", "ok", "Motor importado.");
    } else {
        emit(app, "import", "ok", "El motor ya estaba importado. Se omite.");
    }

    // Step 4 — start the daemon
    emit(app, "start", "running", "Arrancando el motor…");
    lifecycle::ensure_running(app, state).await?;
    emit(app, "start", "ok", "Motor en ejecución.");

    // Step 5 — verify with hello-world (non-fatal if offline)
    emit(app, "verify", "running", "Verificando con un contenedor de prueba…");
    match verify_hello(state).await {
        Ok(_) => emit(app, "verify", "ok", "Verificación correcta."),
        Err(e) => emit(
            app,
            "verify",
            "info",
            &format!("No se pudo verificar con hello-world: {e}"),
        ),
    }

    emit(app, "done", "ok", "✅ Listo para correr contenedores.");
    Ok(SetupResult {
        ok: true,
        needs_reboot: false,
        message: "LiteDock está listo.".into(),
    })
}

/// Enable WSL2 underneath the user with a single elevated command.
/// Returns whether a reboot is required to finish.
async fn install_wsl() -> AppResult<bool> {
    // `wsl --install --no-distribution` enables the features + kernel without
    // pulling Ubuntu. It needs elevation, so we launch it via a UAC prompt.
    let script = "Start-Process -FilePath wsl.exe -ArgumentList '--install','--no-distribution' -Verb RunAs -Wait";
    let (ok, _o, err) = run_powershell(script).await?;
    if !ok {
        return Err(AppError::setup(format!(
            "no se pudo habilitar WSL2: {err}. Ejecuta manualmente `wsl --install`."
        )));
    }
    // If WSL2 still isn't ready after enabling, a reboot is needed.
    let after = detect::detect().await;
    Ok(!after.wsl2_ready)
}

async fn run_powershell(script: &str) -> AppResult<(bool, String, String)> {
    let out = wsl::command("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .await
        .map_err(|e| AppError::setup(format!("no se pudo ejecutar PowerShell: {e}")))?;
    Ok((
        out.status.success(),
        wsl::decode_console(&out.stdout),
        wsl::decode_console(&out.stderr),
    ))
}

/// Pull + run hello-world to prove the engine works end to end.
async fn verify_hello(state: &AppState) -> AppResult<()> {
    use bollard::container::{Config, CreateContainerOptions, WaitContainerOptions};
    use futures_util::StreamExt;

    let docker = state.docker().await?;

    let pull = crate::docker::images::pull_stream(&docker, "hello-world", "latest");
    futures_util::pin_mut!(pull);
    while let Some(item) = pull.next().await {
        item?; // propagate (e.g. offline) errors
    }

    // Clean up any stale instance, then create + run.
    let _ = crate::docker::containers::remove(&docker, "litedock-hello", true).await;
    let opts = CreateContainerOptions {
        name: "litedock-hello".to_string(),
        platform: None,
    };
    let cfg = Config::<String> {
        image: Some("hello-world:latest".to_string()),
        ..Default::default()
    };
    let created = docker.create_container(Some(opts), cfg).await?;
    docker
        .start_container(
            &created.id,
            None::<bollard::container::StartContainerOptions<String>>,
        )
        .await?;
    let wait = docker.wait_container(&created.id, None::<WaitContainerOptions<String>>);
    futures_util::pin_mut!(wait);
    while wait.next().await.is_some() { /* drain until exit */ }
    let _ = crate::docker::containers::remove(&docker, &created.id, true).await;
    Ok(())
}
