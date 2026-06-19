//! Network operations via the Docker Engine API.

use bollard::network::{
    CreateNetworkOptions, InspectNetworkOptions, ListNetworksOptions, PruneNetworksOptions,
};
use bollard::Docker;

use super::types::NetworkDto;
use crate::error::AppResult;

pub async fn list(docker: &Docker) -> AppResult<Vec<NetworkDto>> {
    let nets = docker
        .list_networks(None::<ListNetworksOptions<String>>)
        .await?;
    Ok(nets.into_iter().map(map_network).collect())
}

fn map_network(n: bollard::models::Network) -> NetworkDto {
    NetworkDto {
        id: n.id.unwrap_or_default(),
        name: n.name.unwrap_or_default(),
        driver: n.driver.unwrap_or_default(),
        scope: n.scope.unwrap_or_default(),
        internal: n.internal.unwrap_or_default(),
        containers: n.containers.map(|c| c.len()).unwrap_or(0),
    }
}

pub async fn create(docker: &Docker, name: &str, driver: &str) -> AppResult<()> {
    let opts = CreateNetworkOptions::<String> {
        name: name.to_string(),
        driver: driver.to_string(),
        ..Default::default()
    };
    docker.create_network(opts).await?;
    Ok(())
}

pub async fn remove(docker: &Docker, name: &str) -> AppResult<()> {
    docker.remove_network(name).await?;
    Ok(())
}

pub async fn prune(docker: &Docker) -> AppResult<serde_json::Value> {
    let r = docker
        .prune_networks(None::<PruneNetworksOptions<String>>)
        .await?;
    Ok(serde_json::to_value(r)?)
}

pub async fn inspect(docker: &Docker, name: &str) -> AppResult<serde_json::Value> {
    let r = docker
        .inspect_network(name, None::<InspectNetworkOptions<String>>)
        .await?;
    Ok(serde_json::to_value(r)?)
}
