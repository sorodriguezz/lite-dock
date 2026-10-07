//! Network operations via the Docker Engine API.

use bollard::container::ListContainersOptions;
use bollard::network::{
    CreateNetworkOptions, InspectNetworkOptions, ListNetworksOptions, PruneNetworksOptions,
};
use bollard::Docker;
use std::collections::HashMap;

use super::types::NetworkDto;
use crate::error::AppResult;

pub async fn list(docker: &Docker) -> AppResult<Vec<NetworkDto>> {
    let nets = docker
        .list_networks(None::<ListNetworksOptions<String>>)
        .await?;
    let counts = attached_counts(docker).await;
    Ok(nets.into_iter().map(|n| map_network(n, &counts)).collect())
}

/// Connected (running) containers per network name. The list endpoint never
/// fills `Containers`, so derive it from a single container listing instead of
/// inspecting every network. Best-effort: empty on error.
async fn attached_counts(docker: &Docker) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    let containers = docker
        .list_containers(None::<ListContainersOptions<String>>)
        .await
        .unwrap_or_default();
    for c in containers {
        if let Some(nets) = c.network_settings.and_then(|s| s.networks) {
            for name in nets.into_keys() {
                *counts.entry(name).or_insert(0) += 1;
            }
        }
    }
    counts
}

fn map_network(n: bollard::models::Network, counts: &HashMap<String, usize>) -> NetworkDto {
    let name = n.name.unwrap_or_default();
    NetworkDto {
        id: n.id.unwrap_or_default(),
        containers: counts.get(&name).copied().unwrap_or(0),
        name,
        driver: n.driver.unwrap_or_default(),
        scope: n.scope.unwrap_or_default(),
        internal: n.internal.unwrap_or_default(),
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
