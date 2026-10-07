<script lang="ts">
  import { onMount } from "svelte";
  import { engine, engineBusy, route, notify, askConfirm, terminalReq, openContainerReq } from "../stores";
  import { api } from "../api";
  import { startEngine } from "../engine";
  import type { Container, DiskUsage, Stats } from "../types";
  import { bytes, publishedPorts, dockerDuration } from "../format";
  import Icon from "../components/Icon.svelte";
  import RunContainer from "./RunContainer.svelte";

  let containers = $state<Container[]>([]);
  let imageCount = $state(0);
  let volumeCount = $state(0);
  let disk = $state<DiskUsage | null>(null);
  let liveStats = $state<Record<string, Stats>>({});
  let cpuHist = $state<number[]>([]);
  let loading = $state(true);
  let freeing = $state(false);
  let showRun = $state(false);
  let statsTimer: ReturnType<typeof setInterval> | undefined;

  async function load() {
    loading = true;
    try {
      engine.set(await api.engineStatus());
    } catch {
      /* ignore */
    }
    if ($engine.running) {
      const [c, i, v, d] = await Promise.allSettled([
        api.listContainers(),
        api.listImages(),
        api.listVolumes(),
        api.diskUsage(),
      ]);
      if (c.status === "fulfilled") containers = c.value;
      if (i.status === "fulfilled") imageCount = i.value.length;
      if (v.status === "fulfilled") volumeCount = v.value.length;
      if (d.status === "fulfilled") disk = d.value;
    }
    loading = false;
    pollStats();
  }

  async function pollStats() {
    const running = containers.filter((c) => c.state === "running");
    const live = new Set(running.map((c) => c.id));
    if (Object.keys(liveStats).some((id) => !live.has(id))) {
      liveStats = Object.fromEntries(Object.entries(liveStats).filter(([id]) => live.has(id)));
    }
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
    cpuHist = [...cpuHist, totalCpu].slice(-30);
  }

  // Reload everything when the engine comes up (it may start after this view
  // mounted, e.g. from the sidebar/tray), and clear when it goes down.
  let wasRunning = false;
  $effect(() => {
    const up = $engine.running;
    if (up && !wasRunning) load();
    if (!up) {
      containers = [];
      liveStats = {};
      cpuHist = [];
      loading = false;
    }
    wasRunning = up;
  });

  onMount(() => {
    // (the effect above does the first load)
    // Keep the running list and the stats live; skip a tick while one is in flight.
    let busy = false;
    statsTimer = setInterval(async () => {
      if (busy || !$engine.running) return;
      busy = true;
      try {
        containers = await api.listContainers();
        await pollStats();
      } catch {
        /* transient */
      } finally {
        busy = false;
      }
    }, 4000);
    return () => clearInterval(statsTimer);
  });

  let runningList = $derived(containers.filter((c) => c.state === "running"));
  let stoppedCount = $derived(containers.length - runningList.length);
  let totalCpu = $derived(runningList.reduce((a, c) => a + (liveStats[c.id]?.cpu_percent ?? 0), 0));
  let totalMem = $derived(runningList.reduce((a, c) => a + (liveStats[c.id]?.mem_usage ?? 0), 0));
  let memShare = $derived($engine.mem_total ? Math.min(100, (totalMem / $engine.mem_total) * 100) : 0);

  // Sparkline points for the total CPU (auto-scaled, never flatter than 10 %).
  let sparkPts = $derived.by(() => {
    if (cpuHist.length < 2) return "";
    const max = Math.max(10, ...cpuHist) * 1.15;
    return cpuHist
      .map((v, i) => `${((i / (cpuHist.length - 1)) * 96).toFixed(1)},${(32 - (v / max) * 30).toFixed(1)}`)
      .join(" ");
  });

  let diskTotal = $derived(disk ? disk.images + disk.build_cache + disk.volumes + disk.containers : 0);
  const pct = (n: number) => (diskTotal ? `${(n / diskTotal) * 100}%` : "0%");

  function cpuWidth(c: Container): string {
    const v = liveStats[c.id]?.cpu_percent ?? 0;
    const cores = $engine.ncpu || 1;
    return `${Math.min(100, v / cores)}%`;
  }

  function openPort(port: number) {
    api.openUrl(`http://localhost:${port}`).catch((e) => notify("error", String(e)));
  }

  function openContainer(c: Container) {
    openContainerReq.set(c.id);
    route.set("containers");
  }

  async function freeSpace() {
    const ok = await askConfirm({
      title: "Liberar espacio",
      message:
        "Se eliminarán las imágenes huérfanas (sin etiqueta) y la caché de build que no se esté usando.\nTus contenedores, imágenes con etiqueta y volúmenes no se tocan.",
      confirmText: "Liberar espacio",
    });
    if (!ok) return;
    freeing = true;
    let reclaimed = 0;
    try {
      const r = (await api.pruneImages()) as { SpaceReclaimed?: number } | null;
      reclaimed += r?.SpaceReclaimed ?? 0;
      reclaimed += (await api.pruneBuildCache()) ?? 0;
      notify("success", reclaimed > 0 ? `Liberados ${bytes(reclaimed)}` : "No había nada que limpiar");
    } catch (e) {
      notify("error", String(e));
    }
    freeing = false;
    load();
  }
</script>

<div class="page-head">
  <span class="ph-icon"><Icon name="panel" /></span>
  <h2>Panel</h2>
  <p class="page-sub">Estado del motor y de tus contenedores, en vivo.</p>
  <div class="grow"></div>
  {#if $engine.running}
    <button class="btn" onclick={load} disabled={loading}>
      {#if loading}<span class="spinner"></span>{:else}<Icon name="refresh" />{/if} Actualizar
    </button>
    <button class="btn primary" onclick={() => (showRun = true)}>
      <Icon name="play" /> Ejecutar contenedor
    </button>
  {/if}
</div>

{#if !$engine.running}
  <div class="empty">
    <span class="empty-ic off"><Icon name="power" /></span>
    <div class="big">El motor está apagado</div>
    Tus contenedores, imágenes y volúmenes siguen guardados. Enciéndelo para volver a usarlos.
    <div class="btn-row">
      <button class="btn primary lg" onclick={startEngine} disabled={!!$engineBusy}>
        {#if $engineBusy === "start"}<span class="spinner"></span> Iniciando…{:else}Iniciar motor{/if}
      </button>
      <button class="btn lg" onclick={() => route.set("config")}>Diagnóstico</button>
    </div>
  </div>
{:else}
  <section class="grid cards" aria-label="Resumen">
    <div class="card">
      <div class="label">Contenedores</div>
      <div class="value">{runningList.length}<small> / {containers.length}</small></div>
      <div class="sub" style="display:flex;gap:12px">
        <span style="display:inline-flex;align-items:center;gap:6px"><span class="dot up" style="box-shadow:none;width:7px;height:7px"></span>{runningList.length} en ejecución</span>
        <span style="display:inline-flex;align-items:center;gap:6px"><span class="dot" style="width:7px;height:7px"></span>{stoppedCount} detenidos</span>
      </div>
    </div>

    <div class="card">
      <div class="label">CPU de contenedores</div>
      <div style="display:flex;align-items:flex-end;gap:12px">
        <div class="value">{totalCpu.toFixed(1)}<small>%</small></div>
        {#if sparkPts}
          <svg class="spark" width="96" height="34" viewBox="0 0 96 34" aria-hidden="true" style="margin-left:auto;margin-bottom:6px">
            <polyline points={sparkPts} fill="none" stroke="var(--accent)" stroke-width="2" stroke-linejoin="round" />
          </svg>
        {/if}
      </div>
      <div class="sub">
        {$engine.ncpu ? `${$engine.ncpu} núcleos disponibles · 100 % = 1 núcleo` : "Suma de los contenedores activos"}
      </div>
    </div>

    <div class="card">
      <div class="label">Memoria de contenedores</div>
      <div class="value">{bytes(totalMem)}</div>
      {#if $engine.mem_total}
        <div class="meter blue" style="margin-top:10px"><i style="width:{memShare}%"></i></div>
        <div class="sub">{memShare.toFixed(0)} % de {bytes($engine.mem_total)} del motor</div>
      {:else}
        <div class="sub">Suma de los contenedores activos</div>
      {/if}
    </div>

    <div class="card">
      <div class="label">Disco usado</div>
      <div class="value">{disk ? bytes(diskTotal) : "—"}</div>
      <div class="sub">
        {#if disk && disk.reclaimable > 0}
          <span style="color:var(--warn)">{bytes(disk.reclaimable)}</span> recuperables
        {:else if disk}
          Nada que limpiar
        {:else}
          {imageCount} imágenes · {volumeCount} volúmenes
        {/if}
      </div>
    </div>
  </section>

  <div class="dash-cols">
    <section class="panel-box dash-main" aria-labelledby="run-h">
      <header>
        <h3 id="run-h">En ejecución</h3>
        <button class="btn link" onclick={() => route.set("containers")}>Ver todos los contenedores</button>
      </header>
      {#if runningList.length === 0}
        <div class="empty" style="padding:36px 20px">
          Ningún contenedor en ejecución.
          <div class="btn-row">
            <button class="btn primary" onclick={() => (showRun = true)}>Ejecutar contenedor</button>
          </div>
        </div>
      {:else}
        <div style="overflow-x:auto">
          <table style="min-width:620px">
            <thead>
              <tr>
                <th>Nombre</th>
                <th>Puertos</th>
                <th style="width:170px">CPU</th>
                <th class="num">Memoria</th>
                <th class="num">Activo</th>
              </tr>
            </thead>
            <tbody>
              {#each runningList as c (c.id)}
                <tr>
                  <td>
                    <div style="display:flex;align-items:center;gap:10px;min-width:0">
                      <span class="dot up" style="box-shadow:none"></span>
                      <div class="cell-main">
                        <button class="row-open ellip" style="max-width:260px;display:block" onclick={() => openContainer(c)}>{c.name}</button>
                        <span class="cell-sub mono ellip" style="max-width:260px">{c.image}</span>
                      </div>
                    </div>
                  </td>
                  <td>
                    {#if publishedPorts(c.ports).length}
                      <div class="chips">
                        {#each publishedPorts(c.ports) as p (p.label)}
                          {#if p.web}
                            <button class="port-chip" title="Abrir http://localhost:{p.port}" onclick={() => openPort(p.port)}>
                              {p.label}<Icon name="external" size={11} />
                            </button>
                          {:else}
                            <span class="port-chip" style="cursor:default">{p.label}</span>
                          {/if}
                        {/each}
                      </div>
                    {:else}
                      <span style="color:var(--faint)">—</span>
                    {/if}
                  </td>
                  <td>
                    <div class="meter-row">
                      <div class="meter"><i style="width:{cpuWidth(c)}"></i></div>
                      <span class="num" style="width:48px">{liveStats[c.id] ? liveStats[c.id].cpu_percent.toFixed(1) + "%" : "—"}</span>
                    </div>
                  </td>
                  <td class="num">{liveStats[c.id] ? bytes(liveStats[c.id].mem_usage) : "—"}</td>
                  <td class="num" style="color:var(--muted)">{dockerDuration(c.status) || "—"}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>

    <div class="dash-side">
      <section class="panel-box" aria-labelledby="disk-h">
        <header>
          <h3 id="disk-h">Espacio en disco</h3>
          <span class="num" style="color:var(--muted);font-size:13px">{disk ? bytes(diskTotal) : ""}</span>
        </header>
        <div class="pb-body">
          {#if disk}
            <div class="disk-bar" aria-hidden="true">
              <span style="width:{pct(disk.images)};background:#2dd4bf"></span>
              <span style="width:{pct(disk.build_cache)};background:#0f766e"></span>
              <span style="width:{pct(disk.volumes)};background:#99f6e4"></span>
              <span style="width:{pct(disk.containers)};background:#134e4a"></span>
            </div>
            <div class="legend">
              <span><span class="sw" style="background:#2dd4bf"></span>Imágenes</span><span class="val">{bytes(disk.images)}</span>
              <span><span class="sw" style="background:#0f766e"></span>Caché de build</span><span class="val">{bytes(disk.build_cache)}</span>
              <span><span class="sw" style="background:#99f6e4"></span>Volúmenes</span><span class="val">{bytes(disk.volumes)}</span>
              <span><span class="sw" style="background:#134e4a;outline:1px solid var(--border-strong)"></span>Contenedores</span><span class="val">{bytes(disk.containers)}</span>
            </div>
            <button class="btn lg" style="width:100%;margin-top:14px" onclick={freeSpace} disabled={freeing}>
              {#if freeing}<span class="spinner"></span>{/if} Liberar espacio…
            </button>
          {:else}
            <div style="color:var(--muted);font-size:13px">Calculando…</div>
          {/if}
        </div>
      </section>

      <section class="panel-box" aria-labelledby="qa-h">
        <header><h3 id="qa-h">Atajos</h3></header>
        <div class="pb-body" style="padding:8px">
          <button class="shortcut" onclick={() => route.set("compose")}>
            <span class="sc-ic"><Icon name="compose" /></span>
            <span><b>Levantar un compose</b><small>Abre un docker-compose.yml</small></span>
          </button>
          <button class="shortcut" onclick={() => route.set("build")}>
            <span class="sc-ic"><Icon name="build" /></span>
            <span><b>Construir imagen</b><small>Desde una carpeta con Dockerfile</small></span>
          </button>
          <button class="shortcut" onclick={() => terminalReq.set({ kind: "engine", at: Date.now() })}>
            <span class="sc-ic"><Icon name="terminal" /></span>
            <span><b>Abrir terminal del motor</b><small>docker CLI dentro de WSL</small></span>
          </button>
        </div>
      </section>
    </div>
  </div>
{/if}

{#if showRun}
  <RunContainer onClose={() => (showRun = false)} onCreated={load} />
{/if}
