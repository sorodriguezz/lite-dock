<script lang="ts">
  import { onMount } from "svelte";
  import { route, engine } from "./lib/stores";
  import { api } from "./lib/api";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import TopBar from "./lib/components/TopBar.svelte";
  import TerminalPanel from "./lib/components/TerminalPanel.svelte";
  import Toasts from "./lib/components/Toasts.svelte";
  import FirstRun from "./lib/views/FirstRun.svelte";
  import Dashboard from "./lib/views/Dashboard.svelte";
  import Containers from "./lib/views/Containers.svelte";
  import Images from "./lib/views/Images.svelte";
  import Volumes from "./lib/views/Volumes.svelte";
  import Networks from "./lib/views/Networks.svelte";
  import Build from "./lib/views/Build.svelte";
  import Compose from "./lib/views/Compose.svelte";
  import Configuracion from "./lib/views/Configuracion.svelte";

  let phase = $state<"loading" | "setup" | "ready">("loading");
  let poll: ReturnType<typeof setInterval> | undefined;

  async function boot() {
    // 1. Is the engine already up?
    try {
      const s = await api.engineStatus();
      engine.set(s);
      if (s.running) {
        phase = "ready";
        return;
      }
    } catch {
      /* engine down */
    }
    // 2. Fully installed but stopped → try a silent start.
    try {
      const d = await api.setupDetect();
      if (d.wsl2_ready && d.distro_imported) {
        try {
          await api.engineStart();
          engine.set(await api.engineStatus());
          phase = "ready";
          return;
        } catch {
          /* fall through to wizard */
        }
      }
    } catch {
      /* fall through */
    }
    // 3. Needs setup.
    phase = "setup";
  }

  async function refreshEngine() {
    try {
      engine.set(await api.engineStatus());
    } catch {
      engine.set({ running: false });
    }
  }

  function onReady() {
    phase = "ready";
  }

  onMount(() => {
    boot();
    poll = setInterval(() => {
      if (phase === "ready") refreshEngine();
    }, 5000);
    return () => clearInterval(poll);
  });
</script>

{#if phase === "loading"}
  <div class="firstrun">
    <div class="spinner" style="width:28px;height:28px"></div>
  </div>
{:else if phase === "setup"}
  <FirstRun {onReady} />
{:else}
  <div class="shell">
    <Sidebar />
    <div class="main">
      <TopBar />
      <div class="content">
        {#if $route === "dashboard"}
          <Dashboard />
        {:else if $route === "containers"}
          <Containers />
        {:else if $route === "images"}
          <Images />
        {:else if $route === "volumes"}
          <Volumes />
        {:else if $route === "networks"}
          <Networks />
        {:else if $route === "build"}
          <Build />
        {:else if $route === "compose"}
          <Compose />
        {:else if $route === "config"}
          <Configuracion />
        {/if}
      </div>
      <TerminalPanel />
    </div>
  </div>
{/if}

<Toasts />
