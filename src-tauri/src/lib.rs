//! LiteDock — Tauri 2 application entry point.
//!
//! The UI is a thin client of the Docker Engine API. This crate wires up the
//! command layer, manages shared state, and runs in the background via a system
//! tray so containers keep running when the window is closed ("Salir" quits).

mod commands;
mod config;
mod docker;
mod error;
mod proxy;
mod state;
mod wsl;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // ── engine / system ───────────────────────────────────────────
            commands::engine::engine_status,
            commands::engine::engine_start,
            commands::engine::engine_stop,
            commands::engine::engine_restart,
            commands::engine::system_df,
            commands::engine::disk_usage,
            commands::engine::prune_build_cache,
            commands::engine::app_usage,
            commands::engine::wsl_config_get,
            commands::engine::wsl_config_apply,
            commands::engine::wsl_list_distros,
            commands::engine::wsl_integration_get,
            commands::engine::wsl_integration_set,
            commands::engine::engine_logs,
            commands::engine::enable_docker_cli,
            commands::engine::disable_docker_cli,
            commands::engine::cli_status,
            commands::engine::engine_update,
            commands::engine::open_path,
            commands::engine::open_url,
            // ── first-run / setup ─────────────────────────────────────────
            commands::setup::setup_detect,
            commands::setup::setup_run,
            commands::setup::engine_reset,
            // ── containers ────────────────────────────────────────────────
            commands::containers::list_containers,
            commands::containers::start_container,
            commands::containers::stop_container,
            commands::containers::restart_container,
            commands::containers::pause_container,
            commands::containers::unpause_container,
            commands::containers::kill_container,
            commands::containers::remove_container,
            commands::containers::inspect_container,
            commands::containers::container_stats,
            commands::containers::container_logs_start,
            commands::containers::container_logs_stop,
            commands::containers::exec_start,
            commands::containers::exec_write,
            commands::containers::exec_kill,
            commands::containers::container_update_limits,
            commands::containers::container_browse,
            commands::containers::container_delete_path,
            commands::containers::container_upload,
            commands::containers::container_download,
            commands::containers::container_download_to_host,
            commands::containers::write_host_file,
            commands::containers::run_container,
            // ── images ────────────────────────────────────────────────────
            commands::images::list_images,
            commands::images::pull_image,
            commands::images::remove_image,
            commands::images::prune_images,
            commands::images::image_history,
            commands::images::inspect_image,
            commands::images::search_images,
            // ── volumes ───────────────────────────────────────────────────
            commands::volumes::list_volumes,
            commands::volumes::create_volume,
            commands::volumes::remove_volume,
            commands::volumes::prune_volumes,
            commands::volumes::inspect_volume,
            // ── networks ──────────────────────────────────────────────────
            commands::networks::list_networks,
            commands::networks::create_network,
            commands::networks::remove_network,
            commands::networks::prune_networks,
            commands::networks::inspect_network,
            // ── build & compose ───────────────────────────────────────────
            commands::build::build_image,
            commands::compose::compose_up,
            commands::compose::compose_down,
            commands::compose::compose_logs,
            commands::compose::compose_ls,
            commands::compose::compose_ps,
            // ── integrated terminal ───────────────────────────────────────
            commands::terminal::terminal_start,
            commands::terminal::terminal_write,
            commands::terminal::terminal_kill,
        ])
        .setup(|app| {
            // Path-translation Docker proxy so external `docker` / `docker compose`
            // get Docker Desktop-style Windows bind paths (C:\… → /mnt/c/…).
            tauri::async_runtime::spawn(crate::proxy::run(
                crate::config::ENGINE_PROXY_PORT,
                crate::config::ENGINE_PORT,
            ));

            // Self-heal a stale DOCKER_HOST left pointing at the direct engine
            // port (bypasses path translation → breaks Windows bind mounts).
            // Cheap and only acts on a stale LiteDock value — never when unset.
            std::thread::spawn(crate::wsl::heal_docker_host);

            // Refresh docker integration in any distro we set up before, so an
            // old/broken shim is auto-replaced by the current mechanism.
            tauri::async_runtime::spawn(crate::wsl::repair_integrations());

            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

            // Tray menu: show window + engine controls + a real quit.
            let show_i = MenuItem::with_id(app, "show", "Mostrar LiteDock", true, None::<&str>)?;
            let start_i = MenuItem::with_id(app, "start", "Iniciar motor", true, None::<&str>)?;
            let stop_i = MenuItem::with_id(app, "stop", "Detener motor", true, None::<&str>)?;
            let restart_i =
                MenuItem::with_id(app, "restart", "Reiniciar motor", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &start_i, &stop_i, &restart_i, &quit_i])?;

            let mut builder = TrayIconBuilder::new()
                .tooltip("LiteDock")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "start" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let st = app.state::<AppState>();
                            let _ = wsl::lifecycle::ensure_running(&app, st.inner()).await;
                        });
                    }
                    "stop" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let st = app.state::<AppState>();
                            let _ = wsl::lifecycle::stop_engine(st.inner()).await;
                        });
                    }
                    "restart" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let st = app.state::<AppState>();
                            let _ = wsl::lifecycle::stop_engine(st.inner()).await;
                            let _ = wsl::lifecycle::ensure_running(&app, st.inner()).await;
                        });
                    }
                    "quit" => {
                        wsl::terminate_sync();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                builder = builder.icon(icon.clone());
            }
            builder.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Close to tray: hide the window and keep the engine running in the
            // background (Docker Desktop-style). "Salir" from the tray quits fully.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running LiteDock");
}

/// Bring the main window back to the foreground (from the tray).
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}
