//! Volume operations via the Docker Engine API.

use bollard::volume::{
    CreateVolumeOptions, ListVolumesOptions, PruneVolumesOptions, RemoveVolumeOptions,
};
use bollard::Docker;

use super::types::VolumeDto;
use crate::error::AppResult;

pub async fn list(docker: &Docker) -> AppResult<Vec<VolumeDto>> {
    let resp = docker
        .list_volumes(None::<ListVolumesOptions<String>>)
        .await?;
    // Serialize to JSON and read Docker-API field names. This keeps us
    // independent of whether bollard models `Volumes` as `Vec` or `Option<Vec>`,
    // and of the `Scope` enum representation.
    let val = serde_json::to_value(&resp)?;
    let arr = val
        .get("Volumes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let out = arr
        .into_iter()
        .map(|o| VolumeDto {
            name: o.get("Name").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
            driver: o.get("Driver").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
            mountpoint: o
                .get("Mountpoint")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string(),
            created_at: o.get("CreatedAt").and_then(|x| x.as_str()).map(|s| s.to_string()),
            scope: o.get("Scope").and_then(|x| x.as_str()).unwrap_or("local").to_string(),
        })
        .collect();
    Ok(out)
}

pub async fn create(docker: &Docker, name: &str, driver: &str) -> AppResult<()> {
    let opts = CreateVolumeOptions::<String> {
        name: name.to_string(),
        driver: driver.to_string(),
        ..Default::default()
    };
    docker.create_volume(opts).await?;
    Ok(())
}

pub async fn remove(docker: &Docker, name: &str, force: bool) -> AppResult<()> {
    docker
        .remove_volume(name, Some(RemoveVolumeOptions { force }))
        .await?;
    Ok(())
}

pub async fn prune(docker: &Docker) -> AppResult<serde_json::Value> {
    let r = docker
        .prune_volumes(None::<PruneVolumesOptions<String>>)
        .await?;
    Ok(serde_json::to_value(r)?)
}

pub async fn inspect(docker: &Docker, name: &str) -> AppResult<serde_json::Value> {
    let r = docker.inspect_volume(name).await?;
    Ok(serde_json::to_value(r)?)
}
