<script lang="ts">
  import { onMount } from "svelte";
  import { route, type Route } from "../stores";
  import { api } from "../api";
  import { bytes } from "../format";

  // LiteDock's own footprint (app + WSL engine), polled live.
  let cpu = $state(0);
  let ram = $state(0);

  async function refreshUsage() {
    try {
      const u = await api.appUsage();
      cpu = u.cpu_percent;
      ram = u.ram_bytes;
    } catch {
      /* ignore */
    }
  }
  onMount(() => {
    refreshUsage();
    const t = setInterval(refreshUsage, 4000);
    return () => clearInterval(t);
  });

  const items: { id: Route; label: string; icon: string }[] = [
    { id: "dashboard", label: "Panel", icon: "M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z" },
    {
      id: "containers",
      label: "Contenedores",
      icon: "M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16zM3.27 6.96 12 12.01l8.73-5.05M12 22.08V12",
    },
    { id: "images", label: "Imágenes", icon: "M12 2 2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" },
    {
      id: "volumes",
      label: "Volúmenes",
      icon: "M12 2c4.42 0 8 1.34 8 3s-3.58 3-8 3-8-1.34-8-3 3.58-3 8-3zM4 5v14c0 1.66 3.58 3 8 3s8-1.34 8-3V5M4 12c0 1.66 3.58 3 8 3s8-1.34 8-3",
    },
    {
      id: "networks",
      label: "Redes",
      icon: "M18 8a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 22a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM8.59 13.51l6.83 3.98M15.41 6.51l-6.82 3.98",
    },
    { id: "build", label: "Build", icon: "M4 17l6-6-6-6M12 19h8" },
    {
      id: "compose",
      label: "Compose",
      icon: "M6 3v12M18 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM15 6a9 9 0 0 1-9 9",
    },
  ];
</script>

<aside class="sidebar">
  <div class="brand">
    <svg class="logo" viewBox="0 0 32 32" fill="none" aria-hidden="true">
      <rect x="1" y="1" width="30" height="30" rx="8" fill="url(#g)" />
      <rect x="9" y="14" width="4" height="4" rx="1" fill="#04130f" />
      <rect x="14" y="14" width="4" height="4" rx="1" fill="#04130f" />
      <rect x="19" y="14" width="4" height="4" rx="1" fill="#04130f" />
      <rect x="11.5" y="9" width="4" height="4" rx="1" fill="#04130f" />
      <rect x="16.5" y="9" width="4" height="4" rx="1" fill="#04130f" />
      <path d="M7 19h18a6 6 0 0 1-6 5h-6a6 6 0 0 1-6-5z" fill="#04130f" />
      <defs>
        <linearGradient id="g" x1="0" y1="0" x2="32" y2="32">
          <stop stop-color="#2dd4bf" />
          <stop offset="1" stop-color="#38bdf8" />
        </linearGradient>
      </defs>
    </svg>
    <div>
      <b>LiteDock</b><br />
      <span>v1.0.0</span>
    </div>
  </div>

  {#each items as it (it.id)}
    <button
      class="nav-item {$route === it.id ? 'active' : ''}"
      onclick={() => route.set(it.id)}
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <path d={it.icon} />
      </svg>
      {it.label}
    </button>
  {/each}

  <div class="nav-spacer"></div>

  <button class="nav-item {$route === 'config' ? 'active' : ''}" onclick={() => route.set('config')}>
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z" />
      <path d="M19.4 13a1.65 1.65 0 0 0 .33 1.82l.05.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-2.82 1.17V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.05a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 8.4l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 2.82 1.17l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9z" />
    </svg>
    Configuración
  </button>

  <div class="footprint" title="Consumo de LiteDock (app + motor WSL)">
    <div class="fp-title">Consumo de LiteDock</div>
    <div class="fp-row"><span>CPU</span><b>{cpu.toFixed(1)}%</b></div>
    <div class="fp-row"><span>RAM</span><b>{bytes(ram)}</b></div>
  </div>
</aside>
