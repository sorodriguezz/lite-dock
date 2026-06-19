// Typed wrappers around Tauri commands + a re-export of the event listener.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Container,
  EngineStatus,
  FileEntry,
  Image,
  Network,
  SearchResult,
  SetupResult,
  Stats,
  Volume,
  WslStatus,
} from "./types";

export const api = {
  // ── engine / system ──────────────────────────────────────────────
  engineStatus: () => invoke<EngineStatus>("engine_status"),
  engineStart: () => invoke<void>("engine_start"),
  engineStop: () => invoke<void>("engine_stop"),
  engineRestart: () => invoke<void>("engine_restart"),
  systemDf: () => invoke<unknown>("system_df"),
  engineLogs: () => invoke<string>("engine_logs"),
  enableDockerCli: () => invoke<string>("enable_docker_cli"),
  disableDockerCli: () => invoke<void>("disable_docker_cli"),
  cliStatus: () => invoke<boolean>("cli_status"),
  engineUpdate: () => invoke<number>("engine_update"),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  appUsage: () => invoke<{ cpu_percent: number; ram_bytes: number }>("app_usage"),
  wslConfigGet: () =>
    invoke<{ memory_mb: number | null; auto_reclaim: boolean }>("wsl_config_get"),
  wslConfigApply: (memoryMb: number | null, autoReclaim: boolean) =>
    invoke<void>("wsl_config_apply", { memoryMb, autoReclaim }),

  // ── setup ────────────────────────────────────────────────────────
  setupDetect: () => invoke<WslStatus>("setup_detect"),
  setupRun: () => invoke<SetupResult>("setup_run"),
  engineReset: () => invoke<void>("engine_reset"),

  // ── containers ───────────────────────────────────────────────────
  listContainers: () => invoke<Container[]>("list_containers"),
  startContainer: (id: string) => invoke<void>("start_container", { id }),
  stopContainer: (id: string) => invoke<void>("stop_container", { id }),
  restartContainer: (id: string) => invoke<void>("restart_container", { id }),
  pauseContainer: (id: string) => invoke<void>("pause_container", { id }),
  unpauseContainer: (id: string) => invoke<void>("unpause_container", { id }),
  killContainer: (id: string) => invoke<void>("kill_container", { id }),
  removeContainer: (id: string, force: boolean) =>
    invoke<void>("remove_container", { id, force }),
  inspectContainer: (id: string) => invoke<unknown>("inspect_container", { id }),
  containerStats: (id: string) => invoke<Stats>("container_stats", { id }),
  logsStart: (id: string, tail?: string) =>
    invoke<void>("container_logs_start", { id, tail }),
  logsStop: (id: string) => invoke<void>("container_logs_stop", { id }),
  execStart: (id: string, cmd?: string[]) =>
    invoke<string>("exec_start", { id, cmd }),
  execWrite: (session: string, data: string) =>
    invoke<void>("exec_write", { session, data }),
  execKill: (session: string) => invoke<void>("exec_kill", { session }),
  containerUpdateLimits: (id: string, memoryMb: number | null, cpus: number | null) =>
    // Tauri maps camelCase JS keys → snake_case Rust params, so this must be
    // `memoryMb` (not `memory_mb`), otherwise the memory limit arrives as None.
    invoke<void>("container_update_limits", { id, memoryMb, cpus }),
  containerBrowse: (id: string, path?: string) =>
    invoke<FileEntry[]>("container_browse", { id, path }),
  containerDeletePath: (id: string, path: string) =>
    invoke<void>("container_delete_path", { id, path }),
  containerUpload: (id: string, destDir: string, hostPath: string) =>
    invoke<void>("container_upload", { id, destDir, hostPath }),
  runContainer: (p: {
    image: string;
    name?: string;
    ports: string[];
    env: string[];
    volumes: string[];
    restart: string;
  }) => invoke<string>("run_container", p),

  // ── images ───────────────────────────────────────────────────────
  listImages: () => invoke<Image[]>("list_images"),
  pullImage: (image: string, tag?: string) =>
    invoke<void>("pull_image", { image, tag }),
  removeImage: (id: string, force: boolean) =>
    invoke<void>("remove_image", { id, force }),
  pruneImages: () => invoke<unknown>("prune_images"),
  imageHistory: (id: string) => invoke<unknown>("image_history", { id }),
  inspectImage: (id: string) => invoke<unknown>("inspect_image", { id }),
  searchImages: (term: string, limit?: number) =>
    invoke<SearchResult[]>("search_images", { term, limit }),

  // ── volumes ──────────────────────────────────────────────────────
  listVolumes: () => invoke<Volume[]>("list_volumes"),
  createVolume: (name: string, driver?: string) =>
    invoke<void>("create_volume", { name, driver }),
  removeVolume: (name: string, force: boolean) =>
    invoke<void>("remove_volume", { name, force }),
  pruneVolumes: () => invoke<unknown>("prune_volumes"),
  inspectVolume: (name: string) => invoke<unknown>("inspect_volume", { name }),

  // ── networks ─────────────────────────────────────────────────────
  listNetworks: () => invoke<Network[]>("list_networks"),
  createNetwork: (name: string, driver?: string) =>
    invoke<void>("create_network", { name, driver }),
  removeNetwork: (name: string) => invoke<void>("remove_network", { name }),
  pruneNetworks: () => invoke<unknown>("prune_networks"),
  inspectNetwork: (name: string) => invoke<unknown>("inspect_network", { name }),

  // ── build & compose ──────────────────────────────────────────────
  buildImage: (context: string, dockerfile: string, tag: string) =>
    invoke<number>("build_image", { context, dockerfile, tag }),
  composeUp: (file: string, project: string) =>
    invoke<number>("compose_up", { file, project }),
  composeDown: (file: string, project: string) =>
    invoke<number>("compose_down", { file, project }),
  composeLogs: (file: string, project: string) =>
    invoke<number>("compose_logs", { file, project }),
  composeLs: () => invoke<unknown>("compose_ls"),
  composePs: (file: string, project: string) =>
    invoke<unknown>("compose_ps", { file, project }),

  // ── integrated terminal ──────────────────────────────────────────
  terminalStart: (kind: "host" | "engine") =>
    invoke<string>("terminal_start", { kind }),
  terminalWrite: (session: string, data: string) =>
    invoke<void>("terminal_write", { session, data }),
  terminalKill: (session: string) => invoke<void>("terminal_kill", { session }),
};

export { listen };
export type { UnlistenFn };
