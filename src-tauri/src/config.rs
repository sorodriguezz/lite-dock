//! Centralised compile-time constants and small helpers.
//!
//! Everything that the rest of the app (and the engine packaging / installer)
//! must agree on lives here. Changing a value here propagates everywhere.

/// The WSL2 distribution name dedicated to LiteDock's engine.
/// Kept separate from the user's own distros (like Docker Desktop's
/// `docker-desktop`).
pub const DISTRO_NAME: &str = "litedock-engine";

/// Host the Docker Engine TCP endpoint binds to inside WSL. It is reachable
/// from Windows through WSL2's localhost forwarding. 127.0.0.1 only — the API
/// is never exposed to the network.
pub const ENGINE_HOST: &str = "127.0.0.1";

/// Dedicated, uncommon port chosen to avoid clashing with other local Docker
/// setups (Docker's conventional 2375/2376).
pub const ENGINE_PORT: u16 = 23750;

/// Path of the bundled rootfs relative to the Tauri resource directory.
pub const ROOTFS_RESOURCE_REL: &str = "resources/litedock-engine.tar";

/// Init script path inside the distro (must match engine/files/litedock-init.sh).
pub const INIT_SCRIPT: &str = "/usr/local/bin/litedock-init.sh";

/// Sub-folder under %LOCALAPPDATA% where the imported distro VHD lives.
pub const DATA_SUBDIR: &str = r"LiteDock\engine";

/// The HTTP base URL bollard connects to.
pub fn engine_http_url() -> String {
    format!("http://{ENGINE_HOST}:{ENGINE_PORT}")
}

/// The `DOCKER_HOST` value the standard docker CLI uses to reach our engine.
pub fn engine_tcp_url() -> String {
    format!("tcp://{ENGINE_HOST}:{ENGINE_PORT}")
}

/// `%LOCALAPPDATA%\LiteDock\engine` — where the distro is imported.
/// Falls back to a temp dir if the env var is missing (should never happen on
/// Windows).
pub fn engine_data_dir() -> std::path::PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    std::path::Path::new(&base).join(DATA_SUBDIR)
}
