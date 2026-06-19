<script lang="ts">
  import { engine, route, guard } from "../stores";
  import { api } from "../api";

  const titles: Record<string, string> = {
    dashboard: "Panel",
    containers: "Contenedores",
    images: "Imágenes",
    volumes: "Volúmenes",
    networks: "Redes",
    build: "Build",
    compose: "Compose",
  };
  let busy = $state(false);
  // Acción en curso, para mostrar "Iniciando…/Deteniendo…/Reiniciando…".
  let action = $state<"" | "start" | "stop" | "restart">("");

  async function refresh() {
    try {
      engine.set(await api.engineStatus());
    } catch {
      engine.set({ running: false });
    }
  }
  async function start() {
    busy = true;
    action = "start";
    await guard(() => api.engineStart(), "Motor iniciado");
    await refresh();
    busy = false;
    action = "";
  }
  async function stop() {
    busy = true;
    action = "stop";
    await guard(() => api.engineStop(), "Motor detenido");
    await refresh();
    busy = false;
    action = "";
  }
  async function restart() {
    busy = true;
    action = "restart";
    await guard(() => api.engineRestart(), "Motor reiniciado");
    await refresh();
    busy = false;
    action = "";
  }
</script>

<div class="topbar">
  <h1>{titles[$route] ?? "LiteDock"}</h1>
  <div class="grow"></div>

  <div class="engine-pill">
    <span class="dot {busy ? 'busy' : $engine.running ? 'up' : 'down'}"></span>
    {#if busy}
      {action === "start" ? "Iniciando…" : action === "stop" ? "Deteniendo…" : action === "restart" ? "Reiniciando…" : "Trabajando…"}
    {:else if $engine.running}
      Motor activo{$engine.version ? ` · v${$engine.version}` : ""}
    {:else}
      Motor detenido
    {/if}
  </div>

  {#if $engine.running}
    <button class="btn" onclick={restart} disabled={busy}>
      {#if busy && action === "restart"}<span class="spinner"></span> Reiniciando…{:else}Reiniciar{/if}
    </button>
    <button class="btn danger" onclick={stop} disabled={busy}>
      {#if busy && action === "stop"}<span class="spinner"></span> Deteniendo…{:else}Detener{/if}
    </button>
  {:else}
    <button class="btn primary" onclick={start} disabled={busy}>
      {#if busy && action === "start"}<span class="spinner"></span> Iniciando…{:else}Iniciar motor{/if}
    </button>
  {/if}
</div>
