//! Thin wrappers around `wsl.exe` plus helpers shared by the WSL submodules.

pub mod bootstrap;
pub mod detect;
pub mod lifecycle;
mod usage;

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
        .map_err(|e| AppError::WslUnavailable(format!("no se pudo ejecutar wsl.exe: {e}")))?;
    let stdout = decode_console(&output.stdout);
    let stderr = decode_console(&output.stderr);
    Ok((output.status.success(), stdout, stderr))
}

/// Read one line from a child-process pipe, decoding it lossily so a non-UTF-8
/// byte (e.g. Windows PowerShell's OEM code page) can't end the stream the way
/// `lines()` does. The trailing `\n` / `\r\n` is stripped. `None` at EOF/error.
pub async fn read_line_lossy<R>(reader: &mut R, buf: &mut Vec<u8>) -> Option<String>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    use tokio::io::AsyncBufReadExt;
    buf.clear();
    match reader.read_until(b'\n', buf).await {
        Ok(0) | Err(_) => None,
        Ok(_) => {
            if buf.ends_with(b"\n") {
                buf.pop();
                if buf.ends_with(b"\r") {
                    buf.pop();
                }
            }
            Some(String::from_utf8_lossy(buf).into_owned())
        }
    }
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

/// Repair a stale LiteDock `DOCKER_HOST` (e.g. the old direct engine port
/// `tcp://127.0.0.1:23750`, a `localhost` variant, or a half-typed `23752`) so
/// it points at the path-translation proxy again. Only rewrites values that are
/// clearly LiteDock's: an unset `DOCKER_HOST` is left alone (the CLI is opt-in
/// and the user may have disabled it, or rely on Docker Desktop's default), as
/// is any other endpoint. Runs once at startup.
pub fn heal_docker_host() {
    let proxy_url = crate::config::engine_tcp_url();
    let raw = read_user_docker_host();
    let v = raw.trim();
    if v.is_empty() || v == proxy_url.as_str() {
        return; // not enabled, or already correct — nothing to do
    }
    let litedock_port = [crate::config::ENGINE_PORT, crate::config::ENGINE_PROXY_PORT]
        .iter()
        .any(|p| v.contains(&p.to_string()));
    let local = v.contains("127.0.0.1")
        || v.contains("localhost")
        || v.chars().all(|c| c.is_ascii_digit()); // bare port
    if litedock_port && local {
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

// App footprint (sidebar meter): measured in-process, see `usage.rs`.
pub use usage::app_usage;

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

/// First value of `key` in `[section]` (a section may appear more than once).
fn ini_get<'a>(
    sections: &'a [(String, Vec<(String, String)>)],
    section: &str,
    key: &str,
) -> Option<&'a str> {
    sections
        .iter()
        .filter(|(n, _)| n.eq_ignore_ascii_case(section))
        .flat_map(|(_, kvs)| kvs.iter())
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

/// Set (or, with `value = None`, remove) `key` in `[section]` by editing the
/// file text line by line: comments, blank lines, unrelated keys and sections
/// are kept verbatim (and the user's spelling of the key). A missing key goes
/// after the last entry of the first such section; a missing section is
/// appended at the end. Duplicates of the key in that section are dropped so
/// the value LiteDock writes is the one WSL uses.
fn ini_upsert(content: &str, section: &str, key: &str, value: Option<&str>) -> String {
    let nl = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let header = |l: &str| {
        let t = l.trim();
        (t.starts_with('[') && t.ends_with(']')).then(|| t[1..t.len() - 1].trim().to_string())
    };
    let key_of = |l: &str| {
        let t = l.trim_start();
        if t.starts_with('#') || t.starts_with(';') {
            return None;
        }
        t.split_once('=').map(|(k, _)| k.trim().to_string())
    };

    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let mut in_section = false;
    let mut in_first = false;
    let mut insert_at: Option<usize> = None;
    let mut done = false;
    let mut i = 0;
    while i < lines.len() {
        if let Some(name) = header(&lines[i]) {
            in_section = name.eq_ignore_ascii_case(section);
            in_first = in_section && insert_at.is_none();
            if in_first {
                insert_at = Some(i + 1);
            }
            i += 1;
            continue;
        }
        if in_section {
            if let Some(k) = key_of(&lines[i]).filter(|k| k.eq_ignore_ascii_case(key)) {
                match value {
                    Some(v) if !done => {
                        let indent_len = lines[i].len() - lines[i].trim_start().len();
                        lines[i] = format!("{}{k}={v}", &lines[i][..indent_len]);
                        done = true;
                    }
                    _ => {
                        lines.remove(i); // removal, or a duplicate
                        continue;
                    }
                }
            }
            if in_first && !lines[i].trim().is_empty() {
                insert_at = Some(i + 1);
            }
        }
        i += 1;
    }

    if let (Some(v), false) = (value, done) {
        match insert_at {
            Some(at) => lines.insert(at, format!("{key}={v}")),
            None => {
                if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    lines.push(String::new());
                }
                lines.push(format!("[{section}]"));
                lines.push(format!("{key}={v}"));
            }
        }
    }
    let mut out = lines.join(nl);
    if !out.is_empty() {
        out.push_str(nl);
    }
    out
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
    let sections = parse_ini(content.trim_start_matches('\u{feff}'));
    let mem = ini_get(&sections, "wsl2", "memory").and_then(parse_mem_to_mb);
    let reclaim = ini_get(&sections, "experimental", "autoMemoryReclaim")
        .map(|v| !v.eq_ignore_ascii_case("disabled"))
        .unwrap_or(false);
    (mem, reclaim)
}

/// Set the memory cap (`[wsl2] memory`) + `[experimental] autoMemoryReclaim` in
/// `.wslconfig`, editing it in place: every other line (comments, blank lines,
/// other keys/sections) is kept as is. `memory_mb = None` removes the cap.
pub fn write_wsl_config(memory_mb: Option<u32>, auto_reclaim: bool) -> AppResult<()> {
    let path = wsl_config_path()
        .ok_or_else(|| AppError::other("no se encontró el perfil de usuario".to_string()))?;
    // Never start from scratch over a file we couldn't read (e.g. UTF-16):
    // that would wipe the user's settings.
    let content = match std::fs::read(&path) {
        Ok(bytes) => String::from_utf8(bytes).map_err(|_| {
            AppError::other(
                "no se pudo leer .wslconfig: guárdalo como UTF-8 e inténtalo de nuevo".to_string(),
            )
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(AppError::other(format!("no se pudo leer .wslconfig: {e}"))),
    };
    let (bom, body) = match content.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", content.as_str()),
    };
    let memory = memory_mb.map(|m| format!("{m}MB"));
    let body = ini_upsert(body, "wsl2", "memory", memory.as_deref());
    let body = ini_upsert(
        &body,
        "experimental",
        "autoMemoryReclaim",
        auto_reclaim.then_some("gradual"),
    );
    std::fs::write(&path, format!("{bom}{body}"))
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

/// The WSL distros that are running right now (`wsl --list --running`).
async fn running_distros() -> Vec<String> {
    match run_wsl(&["--list", "--running", "--quiet"]).await {
        Ok((_ok, out, _)) => out
            .lines()
            .map(|l| {
                l.trim_matches(|c: char| c.is_whitespace() || c == '\u{0}')
                    .to_string()
            })
            .filter(|l| !l.is_empty())
            .collect(),
        Err(_) => vec![],
    }
}

/// Whether our `docker` shim is in `distro`: `None` if the probe itself failed.
/// Note: `wsl -d` boots the distro if it was stopped.
async fn shim_state(distro: &str) -> Option<bool> {
    match run_in_distro(
        distro,
        "grep -q litedock /usr/local/bin/docker 2>/dev/null && echo yes || echo no",
    )
    .await
    {
        // Exact lines only: wsl.exe's own errors (e.g. "...not received...")
        // land on stdout too and must not read as a "no".
        Ok((_ok, out, _)) => out
            .lines()
            .map(str::trim)
            .find(|l| *l == "yes" || *l == "no")
            .map(|l| l == "yes"),
        Err(_) => None,
    }
}

/// True if our `docker` shim is installed in `distro`. Also syncs the record of
/// integrated distros (adopts shims installed before the record existed, drops
/// removed ones) — the settings UI probes every distro through this anyway.
pub async fn integration_check(distro: &str) -> bool {
    let state = shim_state(distro).await;
    match state {
        // Unknown revision (0) → refreshed once by the next startup repair.
        Some(true) => update_integrations(|r| {
            if r.distros.contains_key(distro) {
                return false;
            }
            r.distros.insert(distro.to_string(), 0);
            true
        }),
        Some(false) => update_integrations(|r| r.distros.remove(distro).is_some()),
        None => {}
    }
    state.unwrap_or(false)
}

/// Bump whenever the shim installed by `integration_enable` changes: the next
/// startup then refreshes it once in every distro integrated with an older one.
const INTEGRATION_REV: u32 = 1;

/// Distros where LiteDock installed its shim → shim revision. Persisted in
/// `config::integrations_file()`; absent on installs from before it existed.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct IntegrationRecord {
    #[serde(default)]
    distros: std::collections::BTreeMap<String, u32>,
}

/// The record, or `None` if there is none yet (or it is unreadable).
fn load_integrations() -> Option<IntegrationRecord> {
    let raw = std::fs::read(crate::config::integrations_file()).ok()?;
    serde_json::from_slice(&raw).ok()
}

/// Read-modify-write the record; `f` returns whether it changed anything
/// (only then is the file written). Serialised: the UI probes distros in parallel.
fn update_integrations(f: impl FnOnce(&mut IntegrationRecord) -> bool) {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let existed = load_integrations();
    let created = existed.is_none();
    let mut record = existed.unwrap_or_default();
    if f(&mut record) || created {
        let path = crate::config::integrations_file();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(&record) {
            let _ = std::fs::write(&path, json);
        }
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
        update_integrations(|r| {
            r.distros.insert(distro.to_string(), INTEGRATION_REV) != Some(INTEGRATION_REV)
        });
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
        update_integrations(|r| r.distros.remove(distro).is_some());
        Ok(())
    } else {
        Err(AppError::other(format!(
            "no se pudo quitar la integración en {distro}: {err}"
        )))
    }
}

/// On startup, refresh the docker integration in the distros LiteDock set up
/// whose shim is older than the current one — so an out-of-date or broken shim
/// (e.g. an old `wsl.exe`-forwarding one that fails with "MZ: not found") gets
/// replaced with no user action. Only recorded distros are touched (normally
/// none needs it → zero `wsl.exe` calls), so the user's other distros are never
/// booted. Runs once per session, best-effort, in the background.
pub async fn repair_integrations() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.swap(true, Ordering::SeqCst) {
        return;
    }

    let Some(record) = load_integrations() else {
        // No record yet (installs from before it existed): only look at the
        // distros already running — never boot one just for this. Each repaired
        // one is recorded by `integration_enable`; stopped ones get adopted when
        // the settings UI probes them (`integration_check`).
        let running = running_distros().await;
        for d in list_distros().await {
            if running.contains(&d) && shim_state(&d).await == Some(true) {
                let _ = integration_enable(&d).await;
            }
        }
        update_integrations(|_| false); // persist the (maybe empty) record
        return;
    };

    let stale: Vec<String> = record
        .distros
        .iter()
        .filter(|(_, rev)| **rev < INTEGRATION_REV)
        .map(|(d, _)| d.clone())
        .collect();
    if stale.is_empty() {
        return;
    }
    let existing = list_distros().await;
    if existing.is_empty() {
        return; // wsl.exe unavailable right now — retry next launch
    }
    for d in stale {
        if !existing.contains(&d) {
            update_integrations(|r| r.distros.remove(&d).is_some()); // distro deleted
            continue;
        }
        match shim_state(&d).await {
            Some(true) => {
                let _ = integration_enable(&d).await; // records the new revision
            }
            Some(false) => {
                update_integrations(|r| r.distros.remove(&d).is_some()); // shim removed by hand
            }
            None => {} // couldn't tell — keep it for the next launch
        }
    }
}
