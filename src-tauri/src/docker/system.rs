//! Engine-wide status and disk usage.

use super::types::EngineStatus;
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
            }
        }
        Err(_) => EngineStatus::default(),
    }
}

/// `docker system df` — disk usage breakdown.
pub async fn df(docker: &Docker) -> crate::error::AppResult<serde_json::Value> {
    let r = docker.df().await?;
    Ok(serde_json::to_value(r)?)
}
