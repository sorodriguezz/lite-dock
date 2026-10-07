//! Path-translation Docker API proxy (Docker Desktop-style compatibility).
//!
//! The Windows `docker` / `docker compose` CLI points at this proxy via
//! `DOCKER_HOST` (ENGINE_PROXY_PORT). The proxy forwards everything to the real
//! dockerd (ENGINE_PORT) but rewrites **Windows bind paths** (`C:\foo` →
//! `/mnt/c/foo`) in `POST /containers/create` and `POST /volumes/create` bodies,
//! so composes written for Docker Desktop work unchanged on LiteDock's WSL2
//! engine. The app's own bollard client talks to dockerd directly, so a bug here
//! can only affect the external CLI — never the LiteDock UI.
//!
//! Design: per connection we run two directions concurrently. server→client is a
//! dumb byte copy (responses never need rewriting). client→server parses each
//! HTTP/1.1 request (Content-Length / chunked / Upgrade), rewrites the create
//! bodies, and switches to a raw tunnel the moment a connection is hijacked
//! (exec/attach). Request framing depends only on the request, so this stays
//! correct across keep-alive and streaming responses.

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};

/// Listen on `proxy_port` (127.0.0.1) and forward to dockerd on `docker_port`.
pub async fn run(proxy_port: u16, docker_port: u16) {
    let listener = match TcpListener::bind(("127.0.0.1", proxy_port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("litedock proxy: could not bind 127.0.0.1:{proxy_port}: {e}");
            return;
        }
    };
    loop {
        match listener.accept().await {
            Ok((client, _)) => {
                tokio::spawn(handle_conn(client, docker_port));
            }
            Err(_) => continue,
        }
    }
}

async fn handle_conn(client: TcpStream, docker_port: u16) {
    let server = match TcpStream::connect(("127.0.0.1", docker_port)).await {
        Ok(s) => s,
        Err(_) => return, // engine down → client just sees a closed connection
    };
    let _ = client.set_nodelay(true);
    let _ = server.set_nodelay(true);

    let (cr, cw) = client.into_split();
    let (sr, sw) = server.into_split();

    // server → client: plain copy (responses are never rewritten).
    let mut down = tokio::spawn(async move {
        let mut sr = sr;
        let mut cw = cw;
        let _ = tokio::io::copy(&mut sr, &mut cw).await;
    });
    // client → server: parse + rewrite request bodies.
    let mut up = tokio::spawn(pump_requests(BufReader::new(cr), sw));

    // End the whole connection as soon as either direction finishes.
    tokio::select! {
        _ = &mut down => { up.abort(); }
        _ = &mut up => { down.abort(); }
    }
}

async fn pump_requests(
    mut reader: BufReader<OwnedReadHalf>,
    mut sw: OwnedWriteHalf,
) -> std::io::Result<()> {
    loop {
        let head = read_head(&mut reader).await?;
        if head.is_empty() {
            break; // client closed the connection
        }
        let head_str = String::from_utf8_lossy(&head).to_string();
        let lower = head_str.to_ascii_lowercase();
        let first_line = head_str.lines().next().unwrap_or("");

        // Hijacked connection (exec/attach): forward the head, then raw-tunnel.
        if lower.contains("\nupgrade:") || lower.contains("connection: upgrade") {
            sw.write_all(&head).await?;
            tokio::io::copy(&mut reader, &mut sw).await?;
            break;
        }

        // Chunked request body (e.g. build context): forward verbatim, no rewrite.
        if lower.contains("transfer-encoding:") && lower.contains("chunked") {
            sw.write_all(&head).await?;
            copy_chunked(&mut reader, &mut sw).await?;
            continue;
        }

        let clen = parse_content_length(&lower).unwrap_or(0);
        let mut body = vec![0u8; clen];
        if clen > 0 {
            reader.read_exact(&mut body).await?;
        }

        let is_create = first_line.starts_with("POST ")
            && (first_line.contains("/containers/create")
                || first_line.contains("/volumes/create"));

        // Local bind volumes (`DriverOpts: { o: bind, device: <path> }`) are NOT
        // auto-created by dockerd, so a Compose project that points at a missing
        // host folder fails later at container start with
        // "failed to populate volume … no such file or directory". Docker Desktop
        // hides this by creating the directory; we do the same here, up front,
        // for the external CLI. Best-effort: any failure is ignored so the
        // request still forwards and surfaces the real error if needed.
        if first_line.starts_with("POST ") && first_line.contains("/volumes/create") {
            if let Some(dir) = bind_device_from_volume_body(&body) {
                ensure_host_dir(&dir).await;
            }
        }

        if is_create {
            if let Some(new_body) = rewrite_body(&body) {
                let new_head = set_content_length(&head_str, new_body.len());
                sw.write_all(new_head.as_bytes()).await?;
                sw.write_all(&new_body).await?;
                continue;
            }
        }

        sw.write_all(&head).await?;
        if clen > 0 {
            sw.write_all(&body).await?;
        }
    }
    Ok(())
}

/// Read an HTTP head (everything up to and including the blank line).
async fn read_head(reader: &mut BufReader<OwnedReadHalf>) -> std::io::Result<Vec<u8>> {
    let mut head = Vec::new();
    loop {
        let mut line = Vec::new();
        let n = reader.read_until(b'\n', &mut line).await?;
        if n == 0 {
            break; // EOF
        }
        let blank = line == b"\r\n" || line == b"\n";
        head.extend_from_slice(&line);
        if blank {
            break;
        }
    }
    Ok(head)
}

/// Copy a chunked body through unchanged, stopping after the terminating chunk.
async fn copy_chunked(
    reader: &mut BufReader<OwnedReadHalf>,
    sw: &mut OwnedWriteHalf,
) -> std::io::Result<()> {
    loop {
        let mut size_line = Vec::new();
        let n = reader.read_until(b'\n', &mut size_line).await?;
        if n == 0 {
            break;
        }
        sw.write_all(&size_line).await?;
        let s = String::from_utf8_lossy(&size_line);
        let hex = s.trim().split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(hex, 16).unwrap_or(0);
        if size == 0 {
            // Final chunk: forward the trailing CRLF and stop.
            let mut trailer = Vec::new();
            reader.read_until(b'\n', &mut trailer).await?;
            sw.write_all(&trailer).await?;
            break;
        }
        let mut data = vec![0u8; size + 2]; // include the trailing CRLF
        reader.read_exact(&mut data).await?;
        sw.write_all(&data).await?;
    }
    Ok(())
}

fn parse_content_length(lower_head: &str) -> Option<usize> {
    for line in lower_head.lines() {
        if let Some(rest) = line.strip_prefix("content-length:") {
            return rest.trim().parse::<usize>().ok();
        }
    }
    None
}

/// Rebuild the head with a new Content-Length value (other headers untouched).
fn set_content_length(head: &str, len: usize) -> String {
    let mut out = String::new();
    for seg in head.split_inclusive("\r\n") {
        if seg.trim_end().to_ascii_lowercase().starts_with("content-length:") {
            out.push_str(&format!("Content-Length: {len}\r\n"));
        } else {
            out.push_str(seg);
        }
    }
    out
}

// ───────────────────────────── path rewriting ─────────────────────────────

/// `mkdir -p <path>` inside the engine distro, as root. Best-effort: the result
/// is ignored so volume creation is never blocked by this.
async fn ensure_host_dir(path: &str) {
    let _ = crate::wsl::run_wsl(&[
        "-d",
        crate::config::DISTRO_NAME,
        "-u",
        "root",
        "--",
        "mkdir",
        "-p",
        path,
    ])
    .await;
}

/// For a `POST /volumes/create` body, return the host directory that must exist
/// when the volume is a `local` bind mount (`DriverOpts: { o: bind, device }`).
/// The path is normalised to the engine's `/mnt/<drive>/…` form. Returns None
/// for non-bind volumes or unparseable bodies.
fn bind_device_from_volume_body(body: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(body).ok()?;
    let opts = v.get("DriverOpts")?;
    let device = opts.get("device").and_then(|d| d.as_str())?;
    let is_bind = opts
        .get("o")
        .and_then(|o| o.as_str())
        .map(|s| s.contains("bind"))
        .unwrap_or(false)
        || opts
            .get("type")
            .and_then(|t| t.as_str())
            .map(|s| s == "none")
            .unwrap_or(false);
    if !is_bind {
        return None;
    }
    Some(win_to_mnt(device).unwrap_or_else(|| device.to_string()))
}

/// `C:\foo\bar` or `C:/foo` → `/mnt/c/foo/bar`. Returns None if not a Windows
/// absolute path.
fn win_to_mnt(s: &str) -> Option<String> {
    let b = s.as_bytes();
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
    {
        let drive = (b[0] as char).to_ascii_lowercase();
        let rest = s[2..].replace('\\', "/");
        Some(format!("/mnt/{drive}{rest}"))
    } else {
        None
    }
}

/// Rewrite the source of a legacy bind string `C:\src:/dst[:mode]`. Also used
/// by the app's own "run container" form (`docker::containers`).
pub(crate) fn rewrite_bind(s: &str) -> Option<String> {
    let b = s.as_bytes();
    if !(b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'\\' || b[2] == b'/'))
    {
        return None;
    }
    // The container destination is an absolute path, so the separator is the
    // ":/" that appears after the drive letter.
    let idx = s[2..].find(":/")?;
    let split = 2 + idx;
    let new_src = win_to_mnt(&s[..split])?;
    Some(format!("{}{}", new_src, &s[split..]))
}

/// Rewrite Windows bind paths inside a create body. Returns the new bytes only
/// if something changed; otherwise None (forward the original untouched).
fn rewrite_body(body: &[u8]) -> Option<Vec<u8>> {
    let mut v: Value = serde_json::from_slice(body).ok()?;
    let mut changed = false;

    // POST /volumes/create → DriverOpts.device (local bind volumes).
    if let Some(dev) = v.get_mut("DriverOpts").and_then(|d| d.get_mut("device")) {
        if let Some(s) = dev.as_str() {
            if let Some(n) = win_to_mnt(s) {
                *dev = Value::String(n);
                changed = true;
            }
        }
    }

    // POST /containers/create → HostConfig.Binds / HostConfig.Mounts[].Source.
    if let Some(hc) = v.get_mut("HostConfig") {
        if let Some(binds) = hc.get_mut("Binds").and_then(|b| b.as_array_mut()) {
            for entry in binds.iter_mut() {
                if let Some(s) = entry.as_str() {
                    if let Some(n) = rewrite_bind(s) {
                        *entry = Value::String(n);
                        changed = true;
                    }
                }
            }
        }
        if let Some(mounts) = hc.get_mut("Mounts").and_then(|m| m.as_array_mut()) {
            for m in mounts.iter_mut() {
                if let Some(src) = m.get_mut("Source") {
                    if let Some(s) = src.as_str() {
                        if let Some(n) = win_to_mnt(s) {
                            *src = Value::String(n);
                            changed = true;
                        }
                    }
                }
            }
        }
    }

    if changed {
        serde_json::to_vec(&v).ok()
    } else {
        None
    }
}
