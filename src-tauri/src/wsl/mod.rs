//! Thin wrappers around `wsl.exe` plus helpers shared by the WSL submodules.

pub mod bootstrap;
pub mod detect;
pub mod lifecycle;

use crate::error::{AppError, AppResult};
use tokio::process::Command;

/// Build a Command that won't flash a console window on Windows.
pub fn command(program: &str) -> Command {
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW — keep wsl.exe / console children invisible.
        // tokio's Command exposes `creation_flags` inherently on Windows.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Run `wsl.exe <args>` and capture (success, decoded_stdout, decoded_stderr).
/// Console management subcommands (`--status`, `--list`) emit UTF-16LE on
/// Windows, so we decode defensively.
pub async fn run_wsl(args: &[&str]) -> AppResult<(bool, String, String)> {
    let output = command("wsl.exe")
        .args(args)
        .output()
        .await
        .map_err(|e| AppError::WslUnavailable(format!("could not run wsl.exe: {e}")))?;
    let stdout = decode_console(&output.stdout);
    let stderr = decode_console(&output.stderr);
    Ok((output.status.success(), stdout, stderr))
}

/// Decode bytes that may be UTF-16LE (wsl.exe management output) or UTF-8
/// (Linux program output).
pub fn decode_console(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    // Strip a UTF-16LE BOM if present.
    let (body, force_u16) = match bytes {
        [0xFF, 0xFE, rest @ ..] => (rest, true),
        _ => (bytes, false),
    };
    // Heuristic: many interleaved NULs => UTF-16LE ASCII text.
    let nul_ratio = {
        let nuls = body.iter().filter(|&&b| b == 0).count();
        if body.is_empty() {
            0.0
        } else {
            nuls as f64 / body.len() as f64
        }
    };
    if force_u16 || nul_ratio > 0.2 {
        let u16s: Vec<u16> = body
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else {
        String::from_utf8_lossy(body).into_owned()
    }
}

/// Translate a Windows path (`C:\a\b`) to its WSL drvfs mount (`/mnt/c/a/b`).
/// Returns the input unchanged if it doesn't look like a drive path.
pub fn win_to_wsl_path(path: &str) -> String {
    let p = path.replace('\\', "/");
    let bytes = p.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        format!("/mnt/{}{}", drive, &p[2..])
    } else {
        p
    }
}

/// Persist `DOCKER_HOST` to the user environment (via `setx`) so the standard
/// `docker` / `docker compose` CLI on Windows talks to LiteDock's engine. The
/// CLI is opt-in — this only runs from the "Habilitar comandos docker" action.
pub fn set_docker_host() {
    let mut c = std::process::Command::new("setx");
    c.args(["DOCKER_HOST", &crate::config::engine_tcp_url()]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = c.status();
}

/// Remove the persisted `DOCKER_HOST` user env var (the "disable CLI" action).
/// Newly opened terminals stop pointing at LiteDock's engine.
pub fn unset_docker_host() {
    let mut c = std::process::Command::new("reg");
    c.args(["delete", "HKCU\\Environment", "/v", "DOCKER_HOST", "/f"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = c.status();
}

/// Whether `DOCKER_HOST` is persisted in the user environment pointing at our
/// engine (i.e. the Windows `docker` CLI is currently wired to LiteDock).
pub fn docker_host_enabled() -> bool {
    let mut c = std::process::Command::new("reg");
    c.args(["query", "HKCU\\Environment", "/v", "DOCKER_HOST"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    let url = crate::config::engine_tcp_url();
    match c.output() {
        Ok(out) => String::from_utf8_lossy(&out.stdout).contains(url.as_str()),
        Err(_) => false,
    }
}

/// Read the persisted (User-scope) `DOCKER_HOST` value, or "" if unset.
fn read_user_docker_host() -> String {
    let mut c = std::process::Command::new("reg");
    c.args(["query", "HKCU\\Environment", "/v", "DOCKER_HOST"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    match c.output() {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            // Line looks like: "    DOCKER_HOST    REG_SZ    <value>"
            s.lines()
                .find(|l| l.contains("DOCKER_HOST"))
                .and_then(|l| l.split("REG_SZ").nth(1))
                .map(|v| v.trim().to_string())
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}

/// Make sure the Windows `docker` CLI points at LiteDock's path-translation
/// proxy — automatically, so the user never has to set `DOCKER_HOST` by hand.
/// We (re)set it when it is unset, malformed, or points at ANY local endpoint:
/// a fresh install (empty), the old direct port (`:23750`), or a half-typed
/// value like just `23752`. A genuine remote Docker endpoint (a real dotted /
/// remote host) is left untouched. Runs once at startup.
pub fn heal_docker_host() {
    let proxy_url = crate::config::engine_tcp_url();
    let raw = read_user_docker_host();
    let v = raw.trim();
    if v == proxy_url.as_str() {
        return; // already correct — nothing to do
    }
    let is_local_or_bad = v.is_empty()
        || v.contains("127.0.0.1")
        || v.contains("localhost")
        || !v.contains('.'); // bare port / malformed / no real host
    if is_local_or_bad {
        set_docker_host();
    }
}

/// Synchronously terminate the engine distro. Used on app shutdown to free the
/// distro's RAM immediately (a quick, blocking `wsl --terminate`).
pub fn terminate_sync() {
    let mut c = std::process::Command::new("wsl.exe");
    c.args(["--terminate", crate::config::DISTRO_NAME]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = c.status();
}

/// Approximate footprint of LiteDock itself: (cpu_percent, ram_bytes), summing
/// our process + its direct children (WebView2) + the WSL engine VM (vmmem).
/// Best-effort via PowerShell; returns (0, 0) if the probe can't run.
pub async fn app_usage() -> (f64, u64) {
    let own = std::process::id();
    let script = format!(
        "$ErrorActionPreference='SilentlyContinue'; $ids=@({own}); \
Get-CimInstance Win32_Process -Filter 'ParentProcessId={own}' | ForEach-Object {{ $ids += $_.ProcessId }}; \
$vmName=if(Get-Process -Name vmmemWSL){{ 'vmmemWSL' }}else{{ 'Vmmem' }}; \
$a1=@(Get-Process -Id $ids)+@(Get-Process -Name $vmName); \
$ram=($a1 | Measure-Object WorkingSet64 -Sum).Sum; \
$c1=($a1 | Measure-Object CPU -Sum).Sum; \
Start-Sleep -Milliseconds 400; \
$a2=@(Get-Process -Id $ids)+@(Get-Process -Name $vmName); \
$c2=($a2 | Measure-Object CPU -Sum).Sum; \
$cores=[Environment]::ProcessorCount; \
$pct=if($cores -gt 0){{ (($c2-$c1)/0.4/$cores)*100 }}else{{ 0 }}; \
[string]::Format([Globalization.CultureInfo]::InvariantCulture,'{{0:0.0}}|{{1}}',$pct,[int64]$ram)"
    );
    match command("powershell.exe")
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .await
    {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout);
            let mut it = s.trim().split('|');
            let cpu = it
                .next()
                .and_then(|x| x.trim().replace(',', ".").parse::<f64>().ok())
                .unwrap_or(0.0);
            let ram = it
                .next()
                .and_then(|x| x.trim().parse::<u64>().ok())
                .unwrap_or(0);
            (cpu, ram)
        }
        Err(_) => (0.0, 0),
    }
}

// ─────────────────────────── .wslconfig (memory) ───────────────────────────

/// Path to the global WSL config (`%USERPROFILE%\.wslconfig`).
fn wsl_config_path() -> Option<std::path::PathBuf> {
    let profile = std::env::var("USERPROFILE").ok()?;
    Some(std::path::Path::new(&profile).join(".wslconfig"))
}

/// Minimal INI parse into ordered `(section, [(key, value)])`. Comments and
/// blank lines are dropped; the first (section-less) entry has name "".
fn parse_ini(content: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut sections: Vec<(String, Vec<(String, String)>)> = vec![(String::new(), Vec::new())];
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            sections.push((line[1..line.len() - 1].trim().to_string(), Vec::new()));
        } else if let Some(eq) = line.find('=') {
            let k = line[..eq].trim().to_string();
            let v = line[eq + 1..].trim().to_string();
            if let Some(last) = sections.last_mut() {
                last.1.push((k, v));
            }
        }
    }
    sections
}

fn render_ini(sections: &[(String, Vec<(String, String)>)]) -> String {
    let mut out = String::new();
    for (name, kvs) in sections {
        if kvs.is_empty() {
            continue; // skip empty sections (including an empty global area)
        }
        if !name.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("[{name}]\n"));
        }
        for (k, v) in kvs {
            out.push_str(&format!("{k}={v}\n"));
        }
    }
    out
}

/// Set/replace (or remove, if `value` is None) a key under a section.
fn ini_set(
    sections: &mut Vec<(String, Vec<(String, String)>)>,
    section: &str,
    key: &str,
    value: Option<String>,
) {
    let sidx = sections
        .iter()
        .position(|(n, _)| n.eq_ignore_ascii_case(section));
    let sidx = match (sidx, &value) {
        (Some(i), _) => i,
        (None, Some(_)) => {
            sections.push((section.to_string(), Vec::new()));
            sections.len() - 1
        }
        (None, None) => return,
    };
    let kvs = &mut sections[sidx].1;
    let kidx = kvs.iter().position(|(k, _)| k.eq_ignore_ascii_case(key));
    match (kidx, value) {
        (Some(i), Some(v)) => kvs[i].1 = v,
        (Some(i), None) => {
            kvs.remove(i);
        }
        (None, Some(v)) => kvs.push((key.to_string(), v)),
        (None, None) => {}
    }
}

fn ini_get<'a>(
    sections: &'a [(String, Vec<(String, String)>)],
    section: &str,
    key: &str,
) -> Option<&'a str> {
    sections
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(section))?
        .1
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

fn parse_mem_to_mb(v: &str) -> Option<u32> {
    let s = v.trim().to_uppercase();
    let num: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let n: f64 = num.parse().ok()?;
    let mb = if s.ends_with("GB") {
        n * 1024.0
    } else if s.ends_with("MB") {
        n
    } else if s.ends_with("KB") {
        n / 1024.0
    } else {
        n // bare number → assume MB
    };
    Some(mb.round() as u32)
}

/// Read the current `(memory_mb, autoMemoryReclaim_on)` from `.wslconfig`.
pub fn read_wsl_config() -> (Option<u32>, bool) {
    let Some(path) = wsl_config_path() else {
        return (None, false);
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return (None, false);
    };
    let sections = parse_ini(&content);
    let mem = ini_get(&sections, "wsl2", "memory").and_then(parse_mem_to_mb);
    let reclaim = ini_get(&sections, "experimental", "autoMemoryReclaim")
        .map(|v| !v.eq_ignore_ascii_case("disabled"))
        .unwrap_or(false);
    (mem, reclaim)
}

/// Merge the memory cap + autoMemoryReclaim into `.wslconfig`, preserving every
/// other key the user may have. `memory_mb = None` removes the cap.
pub fn write_wsl_config(memory_mb: Option<u32>, auto_reclaim: bool) -> AppResult<()> {
    let path = wsl_config_path()
        .ok_or_else(|| AppError::other("no se encontró el perfil de usuario".to_string()))?;
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut sections = parse_ini(&content);
    ini_set(
        &mut sections,
        "wsl2",
        "memory",
        memory_mb.map(|m| format!("{m}MB")),
    );
    ini_set(
        &mut sections,
        "experimental",
        "autoMemoryReclaim",
        if auto_reclaim {
            Some("gradual".to_string())
        } else {
            None
        },
    );
    std::fs::write(&path, render_ini(&sections))
        .map_err(|e| AppError::other(format!("no se pudo escribir .wslconfig: {e}")))?;
    Ok(())
}

/// Full WSL shutdown so `.wslconfig` is re-read on next start. Affects all distros.
pub async fn shutdown_all() {
    let _ = command("wsl.exe").args(["--shutdown"]).status().await;
}

// ─────────────────────── WSL integration (Docker Desktop-style) ───────────────────────

/// Minimal base64 (so a shell script can be transported as one quote-free arg
/// through `wsl.exe`, then decoded inside the distro — avoids Windows arg-quoting
/// mangling the script's quotes/newlines).
fn b64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(T[(b0 >> 2) as usize] as char);
        out.push(T[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(b2 & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Run a (possibly multi-line) shell script inside `distro` as root, transported
/// base64-encoded so quoting survives the trip through wsl.exe.
async fn run_in_distro(distro: &str, script: &str) -> AppResult<(bool, String, String)> {
    let cmd = format!("echo {} | base64 -d | sh", b64(script.as_bytes()));
    run_wsl(&["-d", distro, "-u", "root", "--", "sh", "-c", cmd.as_str()]).await
}

/// List the user's WSL distros that can host the integration (excludes our engine
/// and Docker Desktop's internal distros).
pub async fn list_distros() -> Vec<String> {
    let skip = ["litedock-engine", "docker-desktop", "docker-desktop-data"];
    match run_wsl(&["--list", "--quiet"]).await {
        Ok((_ok, out, _)) => out
            .lines()
            .map(|l| {
                l.trim()
                    .trim_matches(|c| c == '\r' || c == '\u{0}')
                    .trim()
                    .to_string()
            })
            .filter(|l| !l.is_empty() && !skip.contains(&l.as_str()))
            .collect(),
        Err(_) => vec![],
    }
}

/// True if our `docker` shim is installed in `distro`.
pub async fn integration_status(distro: &str) -> bool {
    match run_in_distro(
        distro,
        "grep -q litedock /usr/local/bin/docker 2>/dev/null && echo yes || echo no",
    )
    .await
    {
        Ok((_ok, out, _)) => out.contains("yes"),
        Err(_) => false,
    }
}

/// Install `docker` + `docker-compose` shims in `distro` that forward to the
/// litedock-engine (so `docker` works inside that distro, like Docker Desktop's
/// WSL integration). Remove with `integration_disable`.
pub async fn integration_enable(distro: &str) -> AppResult<()> {
    // Make `docker` work INSIDE `distro` WITHOUT WSL interop (which fails on some
    // setups with "MZ: not found"). All WSL2 distros share the same localhost, so
    // the engine's TCP port is reachable from here. We install a real, static
    // docker CLI in the distro and a tiny wrapper that points it at the engine.
    // `__PORT__` is substituted with the engine port; transported base64-encoded
    // so it survives the trip through wsl.exe arg-quoting.
    let template = r#"mkdir -p /usr/local/bin /usr/local/lib/litedock
rm -f /usr/local/bin/docker /usr/local/bin/docker-compose
arch=$(uname -m); case "$arch" in aarch64|arm64) a=aarch64;; *) a=x86_64;; esac
dl() { if command -v curl >/dev/null 2>&1; then curl -fsSL "$1" -o "$2"; else wget -qO "$2" "$1"; fi; }
if [ ! -x /usr/local/lib/litedock/docker ]; then
  tmp=$(mktemp -d); got=""
  for v in 27.5.1 27.3.1 26.1.4 25.0.5 24.0.9; do
    if dl "https://download.docker.com/linux/static/stable/$a/docker-$v.tgz" "$tmp/d.tgz"; then got="$v"; break; fi
  done
  if [ -z "$got" ]; then echo "litedock: no pude descargar el docker CLI (revisa la conexion)"; rm -rf "$tmp"; exit 1; fi
  if ! tar -xzf "$tmp/d.tgz" -C "$tmp" docker/docker; then echo "litedock: archivo descargado invalido"; rm -rf "$tmp"; exit 1; fi
  mv "$tmp/docker/docker" /usr/local/lib/litedock/docker
  chmod +x /usr/local/lib/litedock/docker
  rm -rf "$tmp"
fi
cat > /usr/local/bin/docker <<'EOF'
#!/bin/sh
export DOCKER_HOST="tcp://127.0.0.1:__PORT__"
exec /usr/local/lib/litedock/docker "$@"
EOF
chmod +x /usr/local/bin/docker
cat > /etc/profile.d/zz-litedock.sh <<'EOF'
# Win over Docker Desktop's docker on PATH (this runs last in /etc/profile.d).
export DOCKER_HOST="tcp://127.0.0.1:__PORT__"
export PATH="/usr/local/lib/litedock:$PATH"
EOF"#;
    let script = template.replace("__PORT__", &crate::config::ENGINE_PORT.to_string());
    let (ok, _o, err) = run_in_distro(distro, &script).await?;
    if ok {
        Ok(())
    } else {
        Err(AppError::other(format!(
            "no se pudo activar la integración en {distro}: {err}"
        )))
    }
}

/// Remove the LiteDock docker shims from `distro`.
pub async fn integration_disable(distro: &str) -> AppResult<()> {
    let (ok, _o, err) = run_in_distro(
        distro,
        "rm -rf /usr/local/bin/docker /usr/local/bin/docker-compose /usr/local/lib/litedock /etc/profile.d/zz-litedock.sh",
    )
    .await?;
    if ok {
        Ok(())
    } else {
        Err(AppError::other(format!(
            "no se pudo quitar la integración en {distro}: {err}"
        )))
    }
}

/// On startup, refresh the docker integration in any distro we previously set up
/// — so an out-of-date or broken shim (e.g. an old `wsl.exe`-forwarding one that
/// fails with "MZ: not found") gets replaced by the current mechanism, with no
/// user action. Runs once per session, best-effort, in the background.
pub async fn repair_integrations() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    for d in list_distros().await {
        if integration_status(&d).await {
            let _ = integration_enable(&d).await;
        }
    }
}
