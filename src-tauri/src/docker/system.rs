//! Engine-wide status and disk usage.

use super::types::{DiskUsageDto, EngineStatus};
use crate::error::{AppError, AppResult};
use bollard::Docker;

/// Returns engine status. Never errors — a down engine simply yields
/// `running: false`.
pub async fn status(docker: &Docker) -> EngineStatus {
    match docker.version().await {
        Ok(v) => {
            let info = docker.info().await.ok();
            EngineStatus {
                running: true,
                version: v.version,
                api_version: v.api_version,
                containers: info.as_ref().and_then(|i| i.containers).map(|c| c as u64),
                images: info.as_ref().and_then(|i| i.images).map(|c| c as u64),
                ncpu: info.as_ref().and_then(|i| i.ncpu),
                mem_total: info.as_ref().and_then(|i| i.mem_total),
            }
        }
        Err(_) => EngineStatus::default(),
    }
}

/// `docker system df` — disk usage breakdown.
pub async fn df(docker: &Docker) -> AppResult<serde_json::Value> {
    let r = docker.df().await?;
    Ok(serde_json::to_value(r)?)
}

/// `docker system df` summarised in bytes (negative / unknown sizes count as 0).
pub async fn disk_usage(docker: &Docker) -> AppResult<DiskUsageDto> {
    let df = docker.df().await?;
    let pos = |v: i64| u64::try_from(v).unwrap_or(0);

    let images = df.images.unwrap_or_default();
    let containers = df.containers.unwrap_or_default();
    let volumes = df.volumes.unwrap_or_default();
    let cache = df.build_cache.unwrap_or_default();
    let cache_size = |b: &bollard::models::BuildCache| pos(b.size.unwrap_or(0));
    let own_cache = || cache.iter().filter(|b| b.shared != Some(true));
    let vol_usage = || volumes.iter().filter_map(|v| v.usage_data.as_ref());

    // Images no container uses: their size minus the layers shared with others.
    let unused_images: u64 = images
        .iter()
        .filter(|i| i.containers == 0)
        .map(|i| pos(i.size - i.shared_size.max(0)))
        .sum();
    let idle_cache: u64 = own_cache()
        .filter(|b| b.in_use != Some(true))
        .map(cache_size)
        .sum();
    let orphan_volumes: u64 = vol_usage()
        .filter(|u| u.ref_count == 0)
        .map(|u| pos(u.size))
        .sum();

    Ok(DiskUsageDto {
        images: pos(df.layers_size.unwrap_or(0)),
        containers: containers.iter().filter_map(|c| c.size_rw).map(pos).sum(),
        volumes: vol_usage().map(|u| pos(u.size)).sum(),
        build_cache: own_cache().map(cache_size).sum(),
        reclaimable: unused_images + idle_cache + orphan_volumes,
    })
}

/// `docker builder prune --all --force`: drop all unused build cache and return
/// the reclaimed bytes. bollard 0.17 has no build-prune API, so this is a plain
/// HTTP/1.1 `POST /build/prune?all=1` straight to dockerd (same endpoint bollard
/// uses). Can take a while on a big cache, hence the generous timeout.
pub async fn prune_build_cache() -> AppResult<u64> {
    use crate::config::{ENGINE_HOST, ENGINE_PORT};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let request = async {
        let mut s = tokio::net::TcpStream::connect((ENGINE_HOST, ENGINE_PORT)).await?;
        let req = format!(
            "POST /build/prune?all=1 HTTP/1.1\r\nHost: {ENGINE_HOST}:{ENGINE_PORT}\r\n\
Content-Length: 0\r\nConnection: close\r\n\r\n"
        );
        s.write_all(req.as_bytes()).await?;
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).await?;
        Ok::<_, std::io::Error>(raw)
    };
    let raw = match tokio::time::timeout(Duration::from_secs(600), request).await {
        Ok(Ok(raw)) => raw,
        Ok(Err(e)) => {
            return Err(AppError::other(format!(
                "no se pudo conectar con el motor Docker: {e}"
            )))
        }
        Err(_) => {
            return Err(AppError::other(
                "la limpieza de la caché de compilación tardó demasiado".to_string(),
            ))
        }
    };

    let sep = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| AppError::other("respuesta inválida del motor Docker".to_string()))?;
    let head = String::from_utf8_lossy(&raw[..sep]).to_ascii_lowercase();
    let body = if head.contains("transfer-encoding: chunked") {
        dechunk(&raw[sep + 4..])
    } else {
        raw[sep + 4..].to_vec()
    };
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    if !(200..300).contains(&status) {
        let msg = json
            .get("message")
            .and_then(|m| m.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("HTTP {status}"));
        return Err(AppError::other(format!(
            "no se pudo limpiar la caché de compilación: {msg}"
        )));
    }
    Ok(json
        .get("SpaceReclaimed")
        .and_then(|v| v.as_u64())
        .unwrap_or(0))
}

/// Decode an HTTP/1.1 chunked body (already fully read).
fn dechunk(mut data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(eol) = data.windows(2).position(|w| w == b"\r\n") {
        let line = String::from_utf8_lossy(&data[..eol]);
        let size =
            usize::from_str_radix(line.split(';').next().unwrap_or("").trim(), 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        let start = eol + 2;
        let end = start.saturating_add(size).min(data.len());
        out.extend_from_slice(&data[start..end]);
        data = &data[end.saturating_add(2).min(data.len())..];
    }
    out
}
