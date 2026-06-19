//! Read-only WSL2 / engine-distro detection. This is what lets LiteDock skip
//! reinstalling WSL or re-importing the engine when they are already present.

use crate::{config, wsl};
use serde::Serialize;

#[derive(Serialize, Clone, Debug, Default)]
pub struct DistroInfo {
    pub name: String,
    pub state: String,   // Running | Stopped
    pub version: String, // "1" | "2"
    pub default: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct WslStatus {
    /// wsl.exe is runnable and the WSL component is present.
    pub wsl_present: bool,
    /// Modern WSL (store build) detected → WSL2 kernel available.
    pub wsl2_ready: bool,
    /// The `litedock-engine` distro is imported as version 2.
    pub distro_imported: bool,
    /// The engine distro is currently Running.
    pub distro_running: bool,
    /// Hardware virtualization (VT-x/AMD-V) appears available — required for WSL2.
    pub virtualization_enabled: bool,
    /// Human-readable summary for the wizard.
    pub message: String,
    pub distros: Vec<DistroInfo>,
}

/// Probe WSL state without changing anything. Safe to call repeatedly.
pub async fn detect() -> WslStatus {
    let mut status = WslStatus::default();

    // 1. Modern WSL (store) → `wsl --version` succeeds and prints a version.
    if let Ok((ok, out, _)) = wsl::run_wsl(&["--version"]).await {
        if ok && out.to_lowercase().contains("wsl") {
            status.wsl_present = true;
            status.wsl2_ready = true;
        }
    }

    // 2. List distros (works on both modern and inbox WSL).
    if let Ok((ok, out, _)) = wsl::run_wsl(&["--list", "--verbose"]).await {
        if ok {
            status.wsl_present = true;
            status.distros = parse_distros(&out);
            if let Some(d) = status
                .distros
                .iter()
                .find(|d| d.name == config::DISTRO_NAME)
            {
                status.distro_imported = d.version == "2";
                status.distro_running = d.state.eq_ignore_ascii_case("running");
                if status.distro_imported {
                    status.wsl2_ready = true;
                }
            }
        }
    }

    status.virtualization_enabled = virtualization_enabled().await;
    status.message = summarize(&status);
    status
}

/// Best-effort probe: is hardware virtualization available for WSL2? True if the
/// hypervisor is already running OR firmware virtualization (VT-x/AMD-V) is on.
/// Defaults to `true` if the probe can't run, so a capable PC is never blocked.
pub async fn virtualization_enabled() -> bool {
    let script = "if ((Get-CimInstance Win32_ComputerSystem).HypervisorPresent -or \
        (Get-CimInstance Win32_Processor | Select-Object -First 1 -ExpandProperty VirtualizationFirmwareEnabled)) \
        { 'true' } else { 'false' }";
    match wsl::command("powershell.exe")
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .await
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_lowercase().contains("true"),
        Err(_) => true,
    }
}

/// Parse `wsl --list --verbose`. Columns: `[*] NAME STATE VERSION`.
/// The header row is localized, so we key off the numeric version column and
/// skip anything that doesn't end in "1"/"2".
fn parse_distros(out: &str) -> Vec<DistroInfo> {
    let mut result = Vec::new();
    for raw in out.lines() {
        let line = raw.trim_matches(|c| c == '\r' || c == '\u{0}');
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let default = trimmed.starts_with('*');
        let cleaned = trimmed.trim_start_matches('*').trim();
        let cols: Vec<&str> = cleaned.split_whitespace().collect();
        if cols.len() < 3 {
            continue;
        }
        let version = cols[cols.len() - 1];
        if version != "1" && version != "2" {
            continue; // header or malformed row
        }
        let state = cols[cols.len() - 2].to_string();
        let name = cols[..cols.len() - 2].join(" ");
        result.push(DistroInfo {
            name,
            state,
            version: version.to_string(),
            default,
        });
    }
    result
}

fn summarize(s: &WslStatus) -> String {
    if !s.wsl_present {
        "WSL no está disponible. Se instalará automáticamente.".into()
    } else if !s.wsl2_ready {
        "WSL está presente, pero WSL2 aún no está listo.".into()
    } else if !s.distro_imported {
        "WSL2 está listo. Falta importar el motor de LiteDock.".into()
    } else if !s.distro_running {
        "Todo listo. El motor está detenido.".into()
    } else {
        "Motor en ejecución.".into()
    }
}
