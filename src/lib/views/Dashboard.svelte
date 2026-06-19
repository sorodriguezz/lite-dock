<script lang="ts">
  import { onMount } from "svelte";
  import { engine } from "../stores";
  import { api } from "../api";
  import type { Container, Image, Volume, Stats } from "../types";
  import { bytes } from "../format";
  import Icon from "../components/Icon.svelte";

  let containers = $state<Container[]>([]);
  let images = $state<Image[]>([]);
  let volumes = $state<Volume[]>([]);
  let liveStats = $state<Record<string, Stats>>({});
  let loading = $state(true);
  let statsTimer: ReturnType<typeof setInterval> | undefined;

  async function load() {
    loading = true;
    try {
      engine.set(await api.engineStatus());
    } catch {
      /* ignore */
    }
    if ($engine.running) {
      try {
        containers = await api.listContainers();
        images = await api.listImages();
        volumes = await api.listVolumes();
      } catch {
        /* ignore */
      }
    }
    loading = false;
    pollStats();
  }

  async function pollStats() {
    const running = containers.filter((c) => c.state === "running");
    await Promise.allSettled(
      running.map(async (c) => {
        try {
          liveStats[c.id] = await api.containerStats(c.id);
          liveStats = { ...liveStats };
        } catch {
          /* container may have stopped */
        }
      }),
    );
  }

  onMount(() => {
    load();
    statsTimer = setInterval(() => {
      if ($engine.running) pollStats();
    }, 4000);
    return () => clearInterval(statsTimer);
  });

  let runningList = $derived(containers.filter((c) => c.state === "running"));
  let totalCpu = $derived(runningList.reduce((a, c) => a + (liveStats[c.id]?.cpu_percent ?? 0), 0));
  let totalMem = $derived(runningList.reduce((a, c) => a + (liveStats[c.id]?.mem_usage ?? 0), 0));
</script>

<div class="page-head">
  <h2>Panel</h2>
  <div class="grow"></div>
  <button class="btn" onclick={load} disabled={loading}>
    {#if loading}<span class="spinner"></span>{/if} Actualizar
  </button>
</div>

{#if !$engine.running}
  <div class="empty">
    <div class="big">El motor está detenido</div>
    Usa “Iniciar motor” arriba a la derecha para empezar a correr contenedores.
  </div>
{:else}
  <div class="grid cards">
    <div class="card">
      <div class="label">Estado del motor</div>
      <div class="value" style="color:var(--ok)">Activo</div>
      <div style="color:var(--faint);font-size:12px;margin-top:4px">v{$engine.version ?? "—"}</div>
    </div>
    <div class="card">
      <div class="label"><span class="card-ic"><Icon name="cpu" size={14} /></span> CPU total</div>
      <div class="value">{totalCpu.toFixed(1)}<small>%</small></div>
      <div style="color:var(--faint);font-size:12px;margin-top:4px">
        {runningList.length} en ejecución
      </div>
    </div>
    <div class="card">
      <div class="label"><span class="card-ic"><Icon name="ram" size={14} /></span> RAM total</div>
      <div class="value" style="font-size:22px">{bytes(totalMem)}</div>
    </div>
    <div class="card">
      <div class="label"><span class="card-ic"><Icon name="image" size={14} /></span> Imágenes</div>
      <div class="value">{images.length}</div>
    </div>
    <div class="card">
      <div class="label"><span class="card-ic"><Icon name="volume" size={14} /></span> Volúmenes</div>
      <div class="value">{volumes.length}</div>
    </div>
  </div>

  <div class="card" style="margin-top:16px">
    <div class="label" style="margin-bottom:10px">Contenedores en ejecución</div>
    {#if runningList.length === 0}
      <div style="color:var(--faint)">Ninguno en ejecución.</div>
    {:else}
      {#each runningList as c (c.id)}
        <div class="run-row">
          <span class="dot up"></span>
          <b style="font-size:13px">{c.name}</b>
          <span class="mono" style="color:var(--faint)">{c.image}</span>
          <span class="grow"></span>
          <span class="mono" style="color:var(--muted)">
            {liveStats[c.id]
              ? liveStats[c.id].cpu_percent.toFixed(1) + "% · " + bytes(liveStats[c.id].mem_usage)
              : "—"}
          </span>
        </div>
      {/each}
    {/if}
  </div>
{/if}
