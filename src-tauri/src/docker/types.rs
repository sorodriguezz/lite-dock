//! Serializable DTOs sent to the frontend. These decouple the UI contract from
//! bollard's exact model shapes.

use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct PortDto {
    pub ip: Option<String>,
    pub private_port: u16,
    pub public_port: Option<u16>,
    #[serde(rename = "type")]
    pub typ: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct ContainerDto {
    pub id: String,
    pub name: String,
    pub image: String,
    /// running | exited | paused | created | restarting | dead
    pub state: String,
    /// Human status, e.g. "Up 3 minutes".
    pub status: String,
    pub ports: Vec<PortDto>,
    pub created: i64,
    /// Compose project (label `com.docker.compose.project`), used to group rows.
    pub compose_project: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ImageDto {
    pub id: String,
    pub tags: Vec<String>,
    pub size: i64,
    pub created: i64,
    pub dangling: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct VolumeDto {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub created_at: Option<String>,
    pub scope: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct NetworkDto {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub internal: bool,
    pub containers: usize,
}

/// Live resource sample for a single container.
#[derive(Serialize, Clone, Debug)]
pub struct StatsDto {
    pub id: String,
    pub cpu_percent: f64,
    pub mem_usage: u64,
    pub mem_limit: u64,
    pub mem_percent: f64,
    /// Cumulative bytes received / transmitted across all interfaces.
    pub net_rx: u64,
    pub net_tx: u64,
    /// Cumulative block I/O bytes read / written.
    pub blk_read: u64,
    pub blk_write: u64,
}

/// An entry in the container file browser.
#[derive(Serialize, Clone, Debug)]
pub struct FileEntryDto {
    pub name: String,
    pub is_dir: bool,
}

/// A Docker Hub search hit.
#[derive(Serialize, Clone, Debug)]
pub struct SearchResultDto {
    pub name: String,
    pub description: String,
    pub stars: i64,
    pub official: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct EngineStatus {
    pub running: bool,
    pub version: Option<String>,
    pub api_version: Option<String>,
    pub containers: Option<u64>,
    pub images: Option<u64>,
    /// CPUs visible to the engine (`docker info` NCPU).
    pub ncpu: Option<i64>,
    /// Total memory visible to the engine, in bytes (`docker info` MemTotal).
    pub mem_total: Option<i64>,
}

/// Disk usage summary in bytes (`docker system df`).
#[derive(Serialize, Clone, Debug, Default)]
pub struct DiskUsageDto {
    pub images: u64,
    pub containers: u64,
    pub volumes: u64,
    pub build_cache: u64,
    /// Unused images + build cache not in use + unreferenced volumes.
    pub reclaimable: u64,
}
