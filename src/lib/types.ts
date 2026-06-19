// DTOs mirroring the Rust backend (src-tauri/src/docker/types.rs & wsl/*).

export interface Port {
  ip?: string;
  private_port: number;
  public_port?: number;
  type: string;
}

export interface Container {
  id: string;
  name: string;
  image: string;
  state: string;
  status: string;
  ports: Port[];
  created: number;
  compose_project?: string | null;
}

export interface Image {
  id: string;
  tags: string[];
  size: number;
  created: number;
  dangling: boolean;
}

export interface Volume {
  name: string;
  driver: string;
  mountpoint: string;
  created_at?: string;
  scope: string;
}

export interface Network {
  id: string;
  name: string;
  driver: string;
  scope: string;
  internal: boolean;
  containers: number;
}

export interface Stats {
  id: string;
  cpu_percent: number;
  mem_usage: number;
  mem_limit: number;
  mem_percent: number;
  net_rx: number;
  net_tx: number;
  blk_read: number;
  blk_write: number;
}

export interface FileEntry {
  name: string;
  is_dir: boolean;
}

export interface SearchResult {
  name: string;
  description: string;
  stars: number;
  official: boolean;
}

export interface EngineStatus {
  running: boolean;
  version?: string;
  api_version?: string;
  containers?: number;
  images?: number;
}

export interface DistroInfo {
  name: string;
  state: string;
  version: string;
  default: boolean;
}

export interface WslStatus {
  wsl_present: boolean;
  wsl2_ready: boolean;
  distro_imported: boolean;
  distro_running: boolean;
  virtualization_enabled: boolean;
  message: string;
  distros: DistroInfo[];
}

export interface SetupResult {
  ok: boolean;
  needs_reboot: boolean;
  message: string;
}

export interface SetupProgress {
  step: string;
  status: "running" | "ok" | "error" | "info";
  message: string;
}

export interface OutputLine {
  line: string;
  stream: "stdout" | "stderr" | "status";
}
