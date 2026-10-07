//! Container operations via the Docker Engine API.

use bollard::container::{
    KillContainerOptions, ListContainersOptions, LogsOptions, RemoveContainerOptions,
    StartContainerOptions, StatsOptions, UpdateContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::Docker;
use futures_util::Stream;

use super::types::{ContainerDto, PortDto, StatsDto};
use crate::error::AppResult;

pub async fn list(docker: &Docker) -> AppResult<Vec<ContainerDto>> {
    let opts = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };
    let summaries = docker.list_containers(Some(opts)).await?;
    Ok(summaries.into_iter().map(map_container).collect())
}

fn map_container(c: bollard::models::ContainerSummary) -> ContainerDto {
    let name = c
        .names
        .as_ref()
        .and_then(|n| n.first())
        .map(|s| s.trim_start_matches('/').to_string())
        .unwrap_or_default();
    let ports = c
        .ports
        .unwrap_or_default()
        .into_iter()
        .map(|p| PortDto {
            ip: p.ip,
            private_port: p.private_port,
            public_port: p.public_port,
            typ: match p.typ {
                Some(bollard::models::PortTypeEnum::UDP) => "udp",
                Some(bollard::models::PortTypeEnum::SCTP) => "sctp",
                _ => "tcp",
            }
            .to_string(),
        })
        .collect();
    let compose_project = c
        .labels
        .as_ref()
        .and_then(|l| l.get("com.docker.compose.project"))
        .filter(|s| !s.is_empty())
        .cloned();
    ContainerDto {
        id: c.id.unwrap_or_default(),
        name,
        image: c.image.unwrap_or_default(),
        state: c.state.unwrap_or_default(),
        status: c.status.unwrap_or_default(),
        ports,
        created: c.created.unwrap_or_default(),
        compose_project,
    }
}

pub async fn start(docker: &Docker, id: &str) -> AppResult<()> {
    docker
        .start_container(id, None::<StartContainerOptions<String>>)
        .await?;
    Ok(())
}

pub async fn stop(docker: &Docker, id: &str) -> AppResult<()> {
    docker.stop_container(id, None).await?;
    Ok(())
}

pub async fn restart(docker: &Docker, id: &str) -> AppResult<()> {
    docker.restart_container(id, None).await?;
    Ok(())
}

pub async fn pause(docker: &Docker, id: &str) -> AppResult<()> {
    docker.pause_container(id).await?;
    Ok(())
}

pub async fn unpause(docker: &Docker, id: &str) -> AppResult<()> {
    docker.unpause_container(id).await?;
    Ok(())
}

pub async fn kill(docker: &Docker, id: &str) -> AppResult<()> {
    docker
        .kill_container(id, None::<KillContainerOptions<String>>)
        .await?;
    Ok(())
}

pub async fn remove(docker: &Docker, id: &str, force: bool) -> AppResult<()> {
    let opts = RemoveContainerOptions {
        force,
        v: false,
        link: false,
    };
    docker.remove_container(id, Some(opts)).await?;
    Ok(())
}

pub async fn inspect(docker: &Docker, id: &str) -> AppResult<serde_json::Value> {
    let r = docker.inspect_container(id, None).await?;
    Ok(serde_json::to_value(r)?)
}

/// Stream of raw log chunks (stdout+stderr) for `id`.
pub fn logs_stream(
    docker: &Docker,
    id: &str,
    follow: bool,
    tail: &str,
) -> impl Stream<Item = Result<bollard::container::LogOutput, bollard::errors::Error>> {
    let opts = LogsOptions::<String> {
        follow,
        stdout: true,
        stderr: true,
        timestamps: false,
        tail: tail.to_string(),
        ..Default::default()
    };
    docker.logs(id, Some(opts))
}

/// Stream of raw stats for `id`.
pub fn stats_stream(
    docker: &Docker,
    id: &str,
) -> impl Stream<Item = Result<bollard::container::Stats, bollard::errors::Error>> {
    let opts = StatsOptions {
        stream: true,
        one_shot: false,
    };
    docker.stats(id, Some(opts))
}

/// One-shot stats sample with a real CPU% (reads two samples ~1s apart, since
/// CPU usage is a delta between consecutive reads).
pub async fn stats_once(docker: &Docker, id: &str) -> AppResult<StatsDto> {
    use futures_util::StreamExt;
    let s = stats_stream(docker, id);
    futures_util::pin_mut!(s);
    let mut last = None;
    for _ in 0..2 {
        match s.next().await {
            Some(Ok(sample)) => last = Some(sample),
            Some(Err(e)) => return Err(e.into()),
            None => break,
        }
    }
    match last {
        Some(sample) => Ok(stats_to_dto(id, &sample)),
        None => Err(crate::error::AppError::other("sin datos de stats")),
    }
}

/// Start an interactive exec session (tty) running `cmd` in container `id`.
/// Returns the exec instance id (needed to stop it later) and its streams.
pub async fn exec_start(
    docker: &Docker,
    id: &str,
    cmd: Vec<String>,
) -> AppResult<(String, StartExecResults)> {
    let exec = docker
        .create_exec(
            id,
            CreateExecOptions::<String> {
                cmd: Some(cmd),
                attach_stdin: Some(true),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                tty: Some(true),
                ..Default::default()
            },
        )
        .await?;
    // `None` uses defaults (attached, not detached) — avoids depending on the
    // exact StartExecOptions field set across bollard versions.
    let started = docker.start_exec(&exec.id, None).await?;
    Ok((exec.id, started))
}

/// Really stop an interactive exec process. Dropping its stdin is not enough:
/// with a TTY dockerd never delivers EOF, so `top`, `vim`, `tail -f` (or the
/// idle shell itself) keep running. Mimics a terminal hang-up: SIGHUP to the
/// exec's whole session, then SIGTERM to its process group + SIGKILL to the
/// leader if it is still alive. Best-effort, never errors.
///
/// The PID from `inspect_exec` is in dockerd's PID namespace (the engine
/// distro), not the container's, so the signals are sent from the distro —
/// a `kill` run inside the container would target the wrong process.
pub async fn exec_terminate(docker: &Docker, exec_id: &str) {
    let running_pid = |i: bollard::models::ExecInspectResponse| {
        if i.running == Some(true) {
            i.pid.filter(|p| *p > 1)
        } else {
            None
        }
    };
    let Some(pid) = docker
        .inspect_exec(exec_id)
        .await
        .ok()
        .and_then(running_pid)
    else {
        return; // already gone
    };
    // `pkill -s` = every process of the exec's session (foreground jobs get
    // their own process group under job control, so `kill -<pgid>` alone
    // would miss them).
    let script = format!(
        "pkill -HUP -s {pid} 2>/dev/null; kill -HUP {pid} 2>/dev/null; sleep 1; kill -TERM -{pid} 2>/dev/null; kill -KILL {pid} 2>/dev/null; true"
    );
    let _ = crate::wsl::run_wsl(&[
        "-d",
        crate::config::DISTRO_NAME,
        "-u",
        "root",
        "--exec", // no extra shell re-parsing the script
        "sh",
        "-c",
        script.as_str(),
    ])
    .await;
}

/// Compute CPU% and memory from a bollard `Stats` sample (Linux semantics,
/// matching `docker stats`).
pub fn stats_to_dto(id: &str, s: &bollard::container::Stats) -> StatsDto {
    let cpu_delta =
        s.cpu_stats.cpu_usage.total_usage as f64 - s.precpu_stats.cpu_usage.total_usage as f64;
    let system_delta = s.cpu_stats.system_cpu_usage.unwrap_or(0) as f64
        - s.precpu_stats.system_cpu_usage.unwrap_or(0) as f64;
    let online_cpus = s
        .cpu_stats
        .online_cpus
        .unwrap_or_else(|| {
            s.cpu_stats
                .cpu_usage
                .percpu_usage
                .as_ref()
                .map(|v| v.len() as u64)
                .unwrap_or(1)
        })
        .max(1) as f64;
    let cpu_percent = if system_delta > 0.0 && cpu_delta > 0.0 {
        (cpu_delta / system_delta) * online_cpus * 100.0
    } else {
        0.0
    };

    // Same as `docker stats`: usage minus the inactive page cache
    // (`total_inactive_file` on cgroup v1, `inactive_file` on v2), only when
    // that value is present and smaller than the usage.
    let raw_usage = s.memory_stats.usage.unwrap_or(0);
    let inactive = s.memory_stats.stats.as_ref().map(|st| match st {
        bollard::container::MemoryStatsStats::V1(v1) => v1.total_inactive_file,
        bollard::container::MemoryStatsStats::V2(v2) => v2.inactive_file,
    });
    let mem_used = match inactive {
        Some(v) if v < raw_usage => raw_usage - v,
        _ => raw_usage,
    };
    let mem_limit = s.memory_stats.limit.unwrap_or(0);
    let mem_percent = if mem_limit > 0 {
        (mem_used as f64 / mem_limit as f64) * 100.0
    } else {
        0.0
    };

    let (net_rx, net_tx) = s
        .networks
        .as_ref()
        .map(|nets| {
            nets.values()
                .fold((0u64, 0u64), |(rx, tx), n| (rx + n.rx_bytes, tx + n.tx_bytes))
        })
        .unwrap_or((0, 0));

    let (blk_read, blk_write) = s
        .blkio_stats
        .io_service_bytes_recursive
        .as_ref()
        .map(|v| {
            v.iter().fold((0u64, 0u64), |(r, w), e| {
                match e.op.to_ascii_lowercase().as_str() {
                    "read" => (r + e.value, w),
                    "write" => (r, w + e.value),
                    _ => (r, w),
                }
            })
        })
        .unwrap_or((0, 0));

    StatsDto {
        id: id.to_string(),
        cpu_percent,
        mem_usage: mem_used,
        mem_limit,
        mem_percent,
        net_rx,
        net_tx,
        blk_read,
        blk_write,
    }
}

/// Apply resource limits to a container. `memory_mb = Some(0)` removes the
/// memory limit; `cpus` is in cores (e.g. 1.5). Prevents a leaky container from
/// eating all host RAM — it gets OOM-killed at the cap instead.
pub async fn update_resources(
    docker: &Docker,
    id: &str,
    memory_mb: Option<i64>,
    cpus: Option<f64>,
) -> AppResult<()> {
    let memory = memory_mb.map(|m| m.saturating_mul(1024 * 1024));
    let nano_cpus = cpus.map(|c| (c * 1_000_000_000.0) as i64);
    let opts = UpdateContainerOptions::<String> {
        memory,
        memory_swap: memory, // == memory: cap the total, no extra swap growth
        nano_cpus,
        ..Default::default()
    };
    docker.update_container(id, opts).await?;
    Ok(())
}

/// Run `cmd` in the container and capture its combined output (used by the
/// file browser).
pub async fn exec_capture(docker: &Docker, id: &str, cmd: Vec<String>) -> AppResult<String> {
    Ok(exec_capture_code(docker, id, cmd).await?.0)
}

/// Like `exec_capture`, plus the command's exit code (`None` if unknown).
async fn exec_capture_code(
    docker: &Docker,
    id: &str,
    cmd: Vec<String>,
) -> AppResult<(String, Option<i64>)> {
    use futures_util::StreamExt;
    let exec = docker
        .create_exec(
            id,
            CreateExecOptions::<String> {
                cmd: Some(cmd),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                ..Default::default()
            },
        )
        .await?;
    let started = docker.start_exec(&exec.id, None).await?;
    let mut out = String::new();
    if let StartExecResults::Attached { mut output, .. } = started {
        while let Some(item) = output.next().await {
            if let Ok(msg) = item {
                let bytes = match msg {
                    bollard::container::LogOutput::StdOut { message }
                    | bollard::container::LogOutput::StdErr { message }
                    | bollard::container::LogOutput::Console { message }
                    | bollard::container::LogOutput::StdIn { message } => message,
                };
                out.push_str(&String::from_utf8_lossy(&bytes));
            }
        }
    }
    // dockerd records the exit code before closing the exec's streams.
    let code = docker
        .inspect_exec(&exec.id)
        .await
        .ok()
        .and_then(|i| i.exit_code);
    Ok((out, code))
}

/// List directory entries inside the container (simple file browser).
pub async fn browse(
    docker: &Docker,
    id: &str,
    path: &str,
) -> AppResult<Vec<super::types::FileEntryDto>> {
    let p = if path.is_empty() { "/" } else { path };
    let quoted = format!("'{}'", p.replace('\'', "'\\''"));
    let script = format!("ls -1Ap -- {quoted} 2>/dev/null");
    let raw = exec_capture(docker, id, vec!["sh".into(), "-c".into(), script]).await?;
    let mut entries: Vec<super::types::FileEntryDto> = raw
        .lines()
        .map(|l| l.trim_end_matches(['\r', '\n']))
        .filter(|l| !l.is_empty())
        .map(|l| {
            let is_dir = l.ends_with('/');
            super::types::FileEntryDto {
                name: l.trim_end_matches('/').to_string(),
                is_dir,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

/// Single-quote a path for safe interpolation into an `sh -c` command.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Delete a file or directory inside the container (recursive).
pub async fn delete_path(docker: &Docker, id: &str, path: &str) -> AppResult<()> {
    let quoted = sh_quote(path);
    let out = exec_capture(
        docker,
        id,
        vec![
            "sh".into(),
            "-c".into(),
            format!("rm -rf -- {quoted} 2>&1 && printf OK"),
        ],
    )
    .await?;
    if out.trim_end().ends_with("OK") {
        Ok(())
    } else {
        let msg = if out.trim().is_empty() {
            "no se pudo borrar el elemento".to_string()
        } else {
            out
        };
        Err(crate::error::AppError::other(msg))
    }
}

/// Read a file (or a directory, as a tar) from the container, base64-encoded so
/// binary content survives the exec capture. Same philosophy as browse/upload:
/// just shell + base64 inside the container, no tar crate / download API / deps.
pub async fn download(docker: &Docker, id: &str, path: &str, is_dir: bool) -> AppResult<String> {
    let cmd = if is_dir {
        let p = path.trim_end_matches('/');
        let (parent, name): (&str, &str) = match p.rfind('/') {
            Some(0) => ("/", &p[1..]),
            Some(i) => (&p[..i], &p[i + 1..]),
            None => (".", p),
        };
        format!(
            "cd {} 2>/dev/null && tar -cf - {} 2>/dev/null | base64",
            sh_quote(parent),
            sh_quote(name)
        )
    } else {
        format!("base64 < {} 2>/dev/null", sh_quote(path))
    };
    let (out, code) = exec_capture_code(docker, id, vec!["sh".into(), "-c".into(), cmd]).await?;
    // A missing `base64`/`tar` or an unreadable path must not look like a
    // successful 0-byte download. `tar | base64` exits with base64's status, so
    // for a dir an empty result is the tell (a real tar archive is never empty).
    if code.is_some_and(|c| c != 0) || (is_dir && out.trim().is_empty()) {
        let detail: String = out.trim().chars().take(300).collect();
        return Err(crate::error::AppError::other(if detail.is_empty() {
            "no se pudo descargar: la ruta no es legible o faltan `base64`/`tar` en el contenedor"
                .to_string()
        } else {
            format!("no se pudo descargar: {detail}")
        }));
    }
    Ok(out)
}

/// Download ONE regular file from the container straight to the Windows path
/// `dest` (created/overwritten), streaming the archive API's tar and writing the
/// single entry's bytes as they arrive — constant memory, no base64 round trip
/// through the webview. Symlinks are followed (a few hops). Returns the number
/// of bytes written.
pub async fn download_to_host(docker: &Docker, id: &str, path: &str, dest: &str) -> AppResult<u64> {
    use crate::error::AppError;

    let mut target = path.to_string();
    for _ in 0..8 {
        match download_entry(docker, id, &target, dest).await {
            Ok(TarEntry::File(n)) => return Ok(n),
            Ok(TarEntry::Link(link)) => target = resolve_link(&target, &link),
            Ok(TarEntry::Dir) => {
                return Err(AppError::other(
                    "no se pudo descargar: la ruta es un directorio".to_string(),
                ))
            }
            Ok(TarEntry::Other) => {
                return Err(AppError::other(
                    "no se pudo descargar: la ruta no es un archivo regular".to_string(),
                ))
            }
            Err(e) => return Err(e),
        }
    }
    Err(AppError::other(
        "no se pudo descargar: demasiados enlaces simbólicos".to_string(),
    ))
}

/// What the first real entry of a `GET /containers/{id}/archive` tar was.
enum TarEntry {
    /// A regular file, already written to the destination (bytes written).
    File(u64),
    /// A symlink, with its (unresolved) target.
    Link(String),
    Dir,
    Other,
}

/// Resolve a symlink target relative to the directory holding the link.
fn resolve_link(link_path: &str, target: &str) -> String {
    if target.starts_with('/') {
        return target.to_string();
    }
    let parent = match link_path.trim_end_matches('/').rfind('/') {
        Some(i) => &link_path[..i],
        None => "",
    };
    format!("{parent}/{target}")
}

/// Parse a tar numeric field: octal ASCII, or GNU base-256 (high bit set).
fn tar_num(field: &[u8]) -> u64 {
    if field.first().is_some_and(|b| b & 0x80 != 0) {
        return field[1..]
            .iter()
            .fold(u64::from(field[0] & 0x7f), |acc, b| {
                (acc << 8) | u64::from(*b)
            });
    }
    field
        .iter()
        .skip_while(|b| **b == b' ')
        .take_while(|b| (b'0'..=b'7').contains(*b))
        .fold(0u64, |acc, b| acc * 8 + u64::from(b - b'0'))
}

/// `size=` record of a PAX extended header (set for files > 8 GiB).
fn pax_size(data: &[u8]) -> Option<u64> {
    String::from_utf8_lossy(data)
        .lines()
        .filter_map(|l| l.split_once(' ').map(|(_, kv)| kv))
        .find_map(|kv| kv.strip_prefix("size=")?.trim().parse().ok())
}

/// Buffered reader over the archive API's byte stream.
struct ArchiveReader<S> {
    stream: S,
    buf: Vec<u8>,
    eof: bool,
}

impl<S, B> ArchiveReader<S>
where
    S: Stream<Item = Result<B, bollard::errors::Error>> + Unpin,
    B: AsRef<[u8]>,
{
    /// Read until at least `n` bytes are buffered (`false` = the archive ended first).
    async fn fill(&mut self, n: usize) -> AppResult<bool> {
        use futures_util::StreamExt;
        while self.buf.len() < n && !self.eof {
            match self.stream.next().await {
                Some(Ok(chunk)) => self.buf.extend_from_slice(chunk.as_ref()),
                Some(Err(e)) => {
                    return Err(crate::error::AppError::other(format!(
                        "no se pudo descargar: {e}"
                    )))
                }
                None => self.eof = true,
            }
        }
        Ok(self.buf.len() >= n)
    }

    /// Take exactly `n` bytes (`None` = the archive ended first).
    async fn take(&mut self, n: usize) -> AppResult<Option<Vec<u8>>> {
        if !self.fill(n).await? {
            return Ok(None);
        }
        Ok(Some(self.buf.drain(..n).collect()))
    }
}

/// Stream the archive of `path` and handle its first real entry: a regular file
/// is written to `dest` (only created once we know it is one); anything else is
/// just reported back.
async fn download_entry(docker: &Docker, id: &str, path: &str, dest: &str) -> AppResult<TarEntry> {
    use crate::error::AppError;
    use bollard::container::DownloadFromContainerOptions;
    use tokio::io::AsyncWriteExt;

    let truncated = || AppError::other("no se pudo descargar: archivo tar incompleto".to_string());
    let opts = DownloadFromContainerOptions {
        path: path.to_string(),
    };
    let mut rd = ArchiveReader {
        stream: Box::pin(docker.download_from_container(id, Some(opts))),
        buf: Vec::new(),
        eof: false,
    };

    let mut pax_len: Option<u64> = None;
    let mut long_link: Option<String> = None;
    loop {
        let header = rd.take(512).await?.ok_or_else(truncated)?;
        if header.iter().all(|b| *b == 0) {
            return Err(truncated()); // end-of-archive marker before any entry
        }
        let size = tar_num(&header[124..136]);
        let typeflag = header[156];
        match typeflag {
            // Small metadata entries that precede the real one: PAX (x/g) and
            // GNU long name (L) / long link target (K). Read them whole.
            b'x' | b'g' | b'L' | b'K' => {
                let padded = usize::try_from(size.div_ceil(512) * 512)
                    .ok()
                    .filter(|n| *n <= 1 << 20)
                    .ok_or_else(truncated)?;
                let data = rd.take(padded).await?.ok_or_else(truncated)?;
                let data = &data[..size as usize];
                match typeflag {
                    b'x' => pax_len = pax_size(data).or(pax_len),
                    b'K' => {
                        let end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
                        long_link = Some(String::from_utf8_lossy(&data[..end]).to_string());
                    }
                    _ => {}
                }
            }
            // Regular file (incl. old-style NUL typeflag / contiguous file).
            b'0' | 0 | b'7' => {
                let file = tokio::fs::File::create(dest).await.map_err(|e| {
                    AppError::other(format!("no se pudo crear el archivo de destino: {e}"))
                })?;
                let mut out = tokio::io::BufWriter::with_capacity(256 * 1024, file);
                let mut remaining = pax_len.unwrap_or(size);
                let mut written = 0u64;
                let result: AppResult<()> = async {
                    while remaining > 0 {
                        if rd.buf.is_empty() && !rd.fill(1).await? {
                            return Err(AppError::other(
                                "no se pudo descargar: la transferencia se cortó".to_string(),
                            ));
                        }
                        let n = rd
                            .buf
                            .len()
                            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
                        out.write_all(&rd.buf[..n]).await.map_err(|e| {
                            AppError::other(format!("no se pudo guardar el archivo: {e}"))
                        })?;
                        rd.buf.drain(..n);
                        remaining -= n as u64;
                        written += n as u64;
                    }
                    out.flush()
                        .await
                        .map_err(|e| AppError::other(format!("no se pudo guardar el archivo: {e}")))
                }
                .await;
                if let Err(e) = result {
                    drop(out);
                    // Never leave a truncated file behind.
                    let _ = tokio::fs::remove_file(dest).await;
                    return Err(e);
                }
                return Ok(TarEntry::File(written));
            }
            b'2' => {
                let link = long_link.take().unwrap_or_else(|| {
                    let raw = &header[157..257];
                    let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
                    String::from_utf8_lossy(&raw[..end]).to_string()
                });
                return Ok(TarEntry::Link(link));
            }
            b'5' => return Ok(TarEntry::Dir),
            _ => return Ok(TarEntry::Other),
        }
    }
}

/// Upload a Windows host file into `dest_dir` inside the container by streaming
/// its bytes to `cat > <dest>` over an exec — no tar archive or extra deps.
pub async fn upload_file(
    docker: &Docker,
    id: &str,
    dest_dir: &str,
    host_path: &str,
) -> AppResult<()> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let bytes = std::fs::read(host_path)
        .map_err(|e| crate::error::AppError::other(format!("no se pudo leer el archivo: {e}")))?;
    let filename = std::path::Path::new(host_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| crate::error::AppError::other("ruta de archivo inválida".to_string()))?;
    let base = if dest_dir.is_empty() || dest_dir == "/" {
        String::new()
    } else {
        dest_dir.trim_end_matches('/').to_string()
    };
    let dest = format!("{base}/{filename}");
    let quoted = sh_quote(&dest);

    let exec = docker
        .create_exec(
            id,
            CreateExecOptions::<String> {
                // `head -c <len>` reads exactly the file's bytes and exits, so we
                // don't depend on stdin EOF (which could hang the exec/upload).
                cmd: Some(vec![
                    "sh".into(),
                    "-c".into(),
                    format!("head -c {} > {quoted}", bytes.len()),
                ]),
                attach_stdin: Some(true),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                ..Default::default()
            },
        )
        .await?;
    let started = docker.start_exec(&exec.id, None).await?;
    let mut err_text = String::new();
    if let StartExecResults::Attached {
        mut output,
        mut input,
    } = started
    {
        input.write_all(&bytes).await?;
        input.flush().await?;
        drop(input); // EOF → `cat` finishes writing the file
        while let Some(item) = output.next().await {
            if let Ok(msg) = item {
                let b = match msg {
                    bollard::container::LogOutput::StdOut { message }
                    | bollard::container::LogOutput::StdErr { message }
                    | bollard::container::LogOutput::Console { message }
                    | bollard::container::LogOutput::StdIn { message } => message,
                };
                err_text.push_str(&String::from_utf8_lossy(&b));
            }
        }
    }
    if err_text.trim().is_empty() {
        Ok(())
    } else {
        Err(crate::error::AppError::other(err_text))
    }
}

/// Split "repo[:tag]" into (repo, tag), defaulting tag to "latest" and not
/// mistaking a registry port (e.g. localhost:5000/img) for a tag.
fn split_image_tag(image: &str) -> (String, String) {
    match image.rsplit_once(':') {
        Some((repo, tag)) if !tag.contains('/') => (repo.to_string(), tag.to_string()),
        _ => (image.to_string(), "latest".to_string()),
    }
}

/// Ensure `image` exists locally, pulling it when `force` is set or it's missing
/// — so creating a container "just works" even for an image you haven't pulled.
async fn ensure_image(docker: &Docker, image: &str, force: bool) -> AppResult<()> {
    use bollard::image::CreateImageOptions;
    use futures_util::StreamExt;

    if !force && docker.inspect_image(image).await.is_ok() {
        return Ok(());
    }
    let (from_image, tag) = split_image_tag(image);
    let opts = CreateImageOptions::<String> {
        from_image,
        tag,
        ..Default::default()
    };
    let stream = docker.create_image(Some(opts), None, None);
    futures_util::pin_mut!(stream);
    while let Some(item) = stream.next().await {
        item?; // surface auth / network / not-found errors
    }
    Ok(())
}

/// Create a container from `image` with the given settings, then start it.
/// `ports` are "[ip:]host:container[/proto]", `env` are "KEY=VALUE", `volumes`
/// are "source:/dest" (Windows sources like `C:\data` become `/mnt/c/data`).
/// The image is pulled first when `pull` is set (or it's missing).
/// `auto_remove` = `docker run --rm` (the container is deleted when it stops).
/// Returns the new container id.
#[allow(clippy::too_many_arguments)]
pub async fn create_and_start(
    docker: &Docker,
    image: &str,
    name: Option<&str>,
    ports: &[String],
    env: &[String],
    volumes: &[String],
    restart: &str,
    pull: bool,
    publish_all: bool,
    auto_remove: bool,
) -> AppResult<String> {
    use bollard::container::{Config, CreateContainerOptions};
    use bollard::models::{HostConfig, PortBinding, RestartPolicy, RestartPolicyNameEnum};
    use std::collections::HashMap;

    // dockerd rejects AutoRemove together with a restart policy; say so up
    // front (before a possibly long pull) instead of surfacing its English error.
    if auto_remove && matches!(restart, "always" | "unless-stopped" | "on-failure") {
        return Err(crate::error::AppError::other(
            "no se puede eliminar el contenedor al detenerse si tiene una política de reinicio"
                .to_string(),
        ));
    }

    ensure_image(docker, image, pull).await?;

    let mut exposed: HashMap<String, HashMap<(), ()>> = HashMap::new();
    let mut bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    for p in ports {
        let (mapping, proto) = match p.split_once('/') {
            Some((m, pr)) => (m, pr),
            None => (p.as_str(), "tcp"),
        };
        // "[ip:]host:container" → the host side may carry a bind IP
        // (e.g. "127.0.0.1:8080:80"); "container" → published random host port.
        let (host_spec, cont_port) = match mapping.rsplit_once(':') {
            Some((h, c)) => (h.trim(), c.trim()),
            None => ("", mapping.trim()),
        };
        let (host_ip, host_port) = match host_spec.rsplit_once(':') {
            Some((ip, port)) => (ip.trim().trim_matches(['[', ']']), port.trim()),
            None => ("", host_spec),
        };
        let host_ip = if host_ip.is_empty() { "0.0.0.0" } else { host_ip };
        if cont_port.is_empty() {
            continue;
        }
        let key = format!("{cont_port}/{proto}");
        exposed.insert(key.clone(), HashMap::new());
        bindings.insert(
            key,
            Some(vec![PortBinding {
                host_ip: Some(host_ip.to_string()),
                host_port: if host_port.is_empty() {
                    None
                } else {
                    Some(host_port.to_string())
                },
            }]),
        );
    }

    let restart_policy = RestartPolicy {
        name: Some(match restart {
            "always" => RestartPolicyNameEnum::ALWAYS,
            "unless-stopped" => RestartPolicyNameEnum::UNLESS_STOPPED,
            "on-failure" => RestartPolicyNameEnum::ON_FAILURE,
            _ => RestartPolicyNameEnum::NO,
        }),
        maximum_retry_count: None,
    };

    // dockerd runs in WSL: translate Windows host paths (C:\… / C:/…) to their
    // drvfs mount, same as the proxy does for the external CLI.
    let binds: Vec<String> = volumes
        .iter()
        .map(|v| crate::proxy::rewrite_bind(v).unwrap_or_else(|| v.clone()))
        .collect();

    let host_config = HostConfig {
        port_bindings: if bindings.is_empty() { None } else { Some(bindings) },
        publish_all_ports: if publish_all { Some(true) } else { None },
        binds: if binds.is_empty() { None } else { Some(binds) },
        restart_policy: Some(restart_policy),
        auto_remove: if auto_remove { Some(true) } else { None },
        ..Default::default()
    };

    let config = Config {
        image: Some(image.to_string()),
        env: if env.is_empty() { None } else { Some(env.to_vec()) },
        exposed_ports: if exposed.is_empty() { None } else { Some(exposed) },
        host_config: Some(host_config),
        ..Default::default()
    };

    let options = name.filter(|n| !n.is_empty()).map(|n| CreateContainerOptions {
        name: n.to_string(),
        platform: None,
    });

    let created = docker.create_container(options, config).await?;
    if let Err(e) = docker
        .start_container(&created.id, None::<StartContainerOptions<String>>)
        .await
    {
        // Don't leave a "Created" leftover behind (e.g. port already in use):
        // a retry with the same name would fail with a name conflict.
        let opts = RemoveContainerOptions {
            force: true,
            v: true, // its anonymous volumes too (named volumes are kept)
            link: false,
        };
        let _ = docker.remove_container(&created.id, Some(opts)).await;
        return Err(e.into());
    }
    Ok(created.id)
}
