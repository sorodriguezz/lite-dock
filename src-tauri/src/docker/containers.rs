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
pub async fn exec_start(
    docker: &Docker,
    id: &str,
    cmd: Vec<String>,
) -> AppResult<StartExecResults> {
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
    Ok(started)
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

    let raw_usage = s.memory_stats.usage.unwrap_or(0);
    let cache = s
        .memory_stats
        .stats
        .as_ref()
        .and_then(|st| match st {
            bollard::container::MemoryStatsStats::V1(v1) => Some(v1.cache),
            bollard::container::MemoryStatsStats::V2(_) => None,
        })
        .unwrap_or(0);
    let mem_used = raw_usage.saturating_sub(cache);
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
    Ok(out)
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
    let raw = exec_capture(docker, id, vec!["sh".into(), "-lc".into(), script]).await?;
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

/// Single-quote a path for safe interpolation into an `sh -lc` command.
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
            "-lc".into(),
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
    exec_capture(docker, id, vec!["sh".into(), "-lc".into(), cmd]).await
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
                    "-lc".into(),
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
/// `ports` are "host:container[/proto]", `env` are "KEY=VALUE", `volumes` are
/// "source:/dest". The image is pulled first when `pull` is set (or it's missing).
/// Returns the new container id.
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
) -> AppResult<String> {
    use bollard::container::{Config, CreateContainerOptions};
    use bollard::models::{HostConfig, PortBinding, RestartPolicy, RestartPolicyNameEnum};
    use std::collections::HashMap;

    ensure_image(docker, image, pull).await?;

    let mut exposed: HashMap<String, HashMap<(), ()>> = HashMap::new();
    let mut bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    for p in ports {
        let (mapping, proto) = match p.split_once('/') {
            Some((m, pr)) => (m, pr),
            None => (p.as_str(), "tcp"),
        };
        let mut parts = mapping.split(':');
        let a = parts.next().unwrap_or("").trim();
        let b = parts.next().map(|s| s.trim());
        // "host:container" → host=a, container=b; "container" → published random host.
        let (host_port, cont_port) = match b {
            Some(c) => (a, c),
            None => ("", a),
        };
        if cont_port.is_empty() {
            continue;
        }
        let key = format!("{cont_port}/{proto}");
        exposed.insert(key.clone(), HashMap::new());
        bindings.insert(
            key,
            Some(vec![PortBinding {
                host_ip: Some("0.0.0.0".to_string()),
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

    let host_config = HostConfig {
        port_bindings: if bindings.is_empty() { None } else { Some(bindings) },
        publish_all_ports: if publish_all { Some(true) } else { None },
        binds: if volumes.is_empty() { None } else { Some(volumes.to_vec()) },
        restart_policy: Some(restart_policy),
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
    docker
        .start_container(&created.id, None::<StartContainerOptions<String>>)
        .await?;
    Ok(created.id)
}
