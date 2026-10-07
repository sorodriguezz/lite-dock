<script lang="ts">
  import { onMount } from "svelte";
  import { route, engine, engineBusy, type Route } from "../stores";
  import { api } from "../api";
  import { bytes } from "../format";
  import { startEngine, stopEngine, restartEngine, busyLabel } from "../engine";
  import { getVersion } from "@tauri-apps/api/app";
  import Icon from "./Icon.svelte";

  // LiteDock's own footprint (app + WSL engine), polled live.
  let cpu = $state(0);
  let ram = $state(0);
  // Counters next to the nav entries.
  let counts = $state<Partial<Record<Route, string>>>({});

  async function refreshUsage() {
    try {
      const u = await api.appUsage();
      cpu = u.cpu_percent;
      ram = u.ram_bytes;
    } catch {
      /* ignore */
    }
  }

  let countsBusy = false;
  async function refreshCounts(full: boolean) {
    if (countsBusy || !$engine.running) return;
    countsBusy = true;
    try {
      const cs = await api.listContainers();
      const up = cs.filter((c) => c.state === "running").length;
      const next: Partial<Record<Route, string>> = { ...counts, containers: `${up}/${cs.length}` };
      const projects = new Set(cs.map((c) => c.compose_project).filter(Boolean));
      next.compose = projects.size ? String(projects.size) : "";
      if (full) {
        const [im, vo, ne] = await Promise.allSettled([api.listImages(), api.listVolumes(), api.listNetworks()]);
        if (im.status === "fulfilled") next.images = String(im.value.length);
        if (vo.status === "fulfilled") next.volumes = String(vo.value.length);
        // Leave out Docker's three built-in networks (bridge, host, none).
        if (ne.status === "fulfilled")
          next.networks = String(ne.value.filter((n) => !["bridge", "host", "none"].includes(n.name)).length);
      }
      counts = next;
    } catch {
      /* engine may be restarting */
    } finally {
      countsBusy = false;
    }
  }

  // Refresh every count as soon as the engine comes up; clear them when it stops.
  $effect(() => {
    if ($engine.running) refreshCounts(true);
    else counts = {};
  });

  let version = $state("");
  getVersion()
    .then((v) => (version = v))
    .catch(() => {});

  onMount(() => {
    refreshUsage();
    let tick = 0;
    const t = setInterval(() => {
      refreshUsage();
      tick++;
      // Containers every 5 s, the slower-changing lists every 30 s.
      refreshCounts(tick % 6 === 0);
    }, 5000);
    return () => clearInterval(t);
  });

  const general: { id: Route; label: string; icon: string }[] = [
    { id: "dashboard", label: "Panel", icon: "panel" },
    { id: "containers", label: "Contenedores", icon: "container" },
    { id: "images", label: "Imágenes", icon: "image" },
    { id: "volumes", label: "Volúmenes", icon: "volume" },
    { id: "networks", label: "Redes", icon: "network" },
  ];
  const create: { id: Route; label: string; icon: string }[] = [
    { id: "build", label: "Build", icon: "build" },
    { id: "compose", label: "Compose", icon: "compose" },
  ];
</script>

{#snippet navItem(it: { id: Route; label: string; icon: string })}
  <button
    class="nav-item {$route === it.id ? 'active' : ''}"
    aria-current={$route === it.id ? "page" : undefined}
    onclick={() => route.set(it.id)}
  >
    <Icon name={it.icon} />
    <span class="nav-text">{it.label}</span>
    {#if counts[it.id]}<span class="nav-count">{counts[it.id]}</span>{/if}
  </button>
{/snippet}

<aside class="sidebar" aria-label="Navegación principal">
  <div class="brand">
    <svg class="logo" viewBox="0 0 32 32" fill="none" aria-hidden="true">
      <rect x="1" y="1" width="30" height="30" rx="8" fill="#2dd4bf" />
      <rect x="9" y="14" width="4" height="4" rx="1" fill="#04130f" />
      <rect x="14" y="14" width="4" height="4" rx="1" fill="#04130f" />
      <rect x="19" y="14" width="4" height="4" rx="1" fill="#04130f" />
      <rect x="11.5" y="9" width="4" height="4" rx="1" fill="#04130f" />
      <rect x="16.5" y="9" width="4" height="4" rx="1" fill="#04130f" />
      <path d="M7 19h18a6 6 0 0 1-6 5h-6a6 6 0 0 1-6-5z" fill="#04130f" />
    </svg>
    <div>
      <b>LiteDock</b><br />
      <span>{version ? `v${version}` : ""}</span>
    </div>
  </div>

  <span class="nav-label">General</span>
  {#each general as it (it.id)}
    {@render navItem(it)}
  {/each}

  <span class="nav-label" style="padding-top:14px">Crear</span>
  {#each create as it (it.id)}
    {@render navItem(it)}
  {/each}

  <div class="nav-spacer"></div>

  {@render navItem({ id: "config", label: "Configuración", icon: "settings" })}

  <section class="engine-card" aria-label="Motor de LiteDock">
    <div class="ec-head">
      <span class="dot {$engineBusy ? 'busy' : $engine.running ? 'up' : 'down'}"></span>
      <span class="ec-title" role="status">
        {#if $engineBusy}
          {busyLabel[$engineBusy]}
        {:else if $engine.running}
          Motor activo
        {:else}
          Motor detenido
        {/if}
      </span>
      {#if $engine.running && $engine.version}
        <span class="ec-version" title="Versión de Docker Engine">{$engine.version}</span>
      {/if}
    </div>
    <div class="ec-usage" title="Consumo de LiteDock: la app y su motor en WSL">
      <span>CPU <b>{cpu.toFixed(1)}%</b></span>
      <span>RAM <b>{bytes(ram)}</b></span>
    </div>
    <div class="ec-actions">
      {#if $engine.running}
        <button class="btn" onclick={restartEngine} disabled={!!$engineBusy}>
          {#if $engineBusy === "restart"}<span class="spinner"></span>{/if} Reiniciar
        </button>
        <button class="btn danger" onclick={stopEngine} disabled={!!$engineBusy}>
          {#if $engineBusy === "stop"}<span class="spinner"></span>{/if} Detener
        </button>
      {:else}
        <button class="btn primary" onclick={startEngine} disabled={!!$engineBusy}>
          {#if $engineBusy === "start"}<span class="spinner"></span>{/if} Iniciar motor
        </button>
      {/if}
    </div>
  </section>
</aside>
