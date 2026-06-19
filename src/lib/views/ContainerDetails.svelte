<script lang="ts">
  import { onMount } from "svelte";
  import Modal from "../components/Modal.svelte";
  import LogConsole from "../components/LogConsole.svelte";
  import { api, listen, type UnlistenFn } from "../api";
  import { notify } from "../stores";
  import type { Container, FileEntry, Stats } from "../types";
  import { stripAnsi, bytes } from "../format";
  import { open } from "@tauri-apps/plugin-dialog";
  import Sparkline from "../components/Sparkline.svelte";
  import Icon from "../components/Icon.svelte";

  interface Props {
    container: Container;
    onClose: () => void;
  }
  let { container, onClose }: Props = $props();

  type Tab = "logs" | "terminal" | "stats" | "files" | "inspect";
  let tab = $state<Tab>("logs");

  let inspectJson = $state("");

  let logLines = $state<string[]>([]);
  let logUnlisten: UnlistenFn | undefined;

  let session = $state<string | null>(null);
  let termLines = $state<string[]>([]);
  let termInput = $state("");
  let execUnlisten: UnlistenFn | undefined;
  let execExitUnlisten: UnlistenFn | undefined;

  let stats = $state<Stats | null>(null);
  let statsTimer: ReturnType<typeof setInterval> | undefined;
  let cpuHist = $state<number[]>([]);
  let memHist = $state<number[]>([]);
  let netHist = $state<number[]>([]);
  let diskHist = $state<number[]>([]);
  let prevNet: number | null = null;
  let prevDisk: number | null = null;
  // Latest + peak throughput (bytes/s) over the window, for the graph labels.
  let netRate = $derived(netHist.length ? netHist[netHist.length - 1] / 2.5 : 0);
  let diskRate = $derived(diskHist.length ? diskHist[diskHist.length - 1] / 2.5 : 0);
  let netPeak = $derived(netHist.length ? Math.max(...netHist) / 2.5 : 0);
  let diskPeak = $derived(diskHist.length ? Math.max(...diskHist) / 2.5 : 0);

  // resource limits
  let memLimit = $state("");
  let cpuLimit = $state("");
  let applyingLimit = $state(false);

  // file browser
  let browsePath = $state("/");
  let browseEntries = $state<FileEntry[]>([]);
  let browseLoading = $state(false);
  let uploading = $state(false);

  onMount(() => {
    startLogs();
    loadLimits();
    return () => stopAll();
  });

  // Pre-fill the limit fields with the container's current limits (persisted by Docker).
  async function loadLimits() {
    try {
      const data = (await api.inspectContainer(container.id)) as {
        HostConfig?: { Memory?: number; NanoCpus?: number };
      };
      const mem = data.HostConfig?.Memory ?? 0;
      const nano = data.HostConfig?.NanoCpus ?? 0;
      if (mem > 0) memLimit = String(Math.round(mem / (1024 * 1024)));
      if (nano > 0) cpuLimit = String(+(nano / 1e9).toFixed(2));
    } catch {
      /* ignore */
    }
  }

  async function setTab(t: Tab) {
    tab = t;
    if (t === "inspect" && !inspectJson) {
      try {
        inspectJson = JSON.stringify(await api.inspectContainer(container.id), null, 2);
      } catch (e) {
        inspectJson = String(e);
      }
    }
    if (t === "terminal" && !session) startTerminal();
    if (t === "stats" && !statsTimer) startStats();
    if (t === "files" && browseEntries.length === 0) loadBrowse("/");
  }

  async function startLogs() {
    logLines = [];
    logUnlisten = await listen<{ id: string; line: string }>("container-log", (e) => {
      if (e.payload.id === container.id) {
        logLines = [...logLines, stripAnsi(e.payload.line)].slice(-2000);
      }
    });
    try {
      await api.logsStart(container.id, "300");
    } catch (e) {
      notify("error", String(e));
    }
  }

  async function startTerminal() {
    termLines = [];
    execUnlisten = await listen<{ session: string; line: string }>("exec-output", (e) => {
      if (e.payload.session === session) {
        termLines = [...termLines, stripAnsi(e.payload.line)].slice(-2000);
      }
    });
    execExitUnlisten = await listen<string>("exec-exit", (e) => {
      if (e.payload === session) {
        termLines = [...termLines, "\n[sesión finalizada]"];
        session = null;
      }
    });
    try {
      session = await api.execStart(container.id, ["/bin/sh"]);
    } catch (e) {
      notify("error", String(e));
    }
  }

  async function sendInput(e: Event) {
    e.preventDefault();
    if (!session) return;
    // `clear`/`cls` emit ANSI escapes that stripAnsi removes, so the buffer
    // never clears. Clear it locally; the shell still prints a fresh prompt.
    const cmd = termInput.trim().toLowerCase();
    if (cmd === "clear" || cmd === "cls") {
      termLines = [];
    }
    const data = termInput + "\n";
    termInput = "";
    try {
      await api.execWrite(session, data);
    } catch (err) {
      notify("error", String(err));
    }
  }

  function startStats() {
    const tick = async () => {
      try {
        const s = await api.containerStats(container.id);
        stats = s;
        // Keep rolling windows for the live graphs (net/disk plot per-tick rate).
        cpuHist = [...cpuHist, s.cpu_percent].slice(-40);
        memHist = [...memHist, s.mem_percent].slice(-40);
        const netTotal = s.net_rx + s.net_tx;
        const diskTotal = s.blk_read + s.blk_write;
        if (prevNet !== null) netHist = [...netHist, Math.max(0, netTotal - prevNet)].slice(-40);
        if (prevDisk !== null) diskHist = [...diskHist, Math.max(0, diskTotal - prevDisk)].slice(-40);
        prevNet = netTotal;
        prevDisk = diskTotal;
      } catch {
        /* container may have stopped */
      }
    };
    tick();
    statsTimer = setInterval(tick, 2500);
  }

  async function applyLimits() {
    applyingLimit = true;
    const mem = memLimit.trim() === "" ? null : Math.max(0, Math.round(Number(memLimit)));
    const cpu = cpuLimit.trim() === "" ? null : Math.max(0, Number(cpuLimit));
    try {
      await api.containerUpdateLimits(container.id, mem, cpu);
      notify("success", "Límites aplicados");
      // Refresh stats so the new limit (the "/ X" denominator) shows immediately.
      try {
        stats = await api.containerStats(container.id);
      } catch {
        /* ignore */
      }
    } catch (e) {
      notify("error", String(e));
    }
    applyingLimit = false;
  }

  async function loadBrowse(path: string) {
    browseLoading = true;
    try {
      browseEntries = await api.containerBrowse(container.id, path);
      browsePath = path;
    } catch (e) {
      notify("error", String(e));
    }
    browseLoading = false;
  }
  function enterDir(name: string) {
    const base = browsePath === "/" ? "" : browsePath.replace(/\/$/, "");
    loadBrowse(`${base}/${name}`);
  }
  function goUp() {
    if (browsePath === "/") return;
    const parts = browsePath.replace(/\/$/, "").split("/");
    parts.pop();
    loadBrowse(parts.join("/") || "/");
  }
  // Extension for the badge; dot at index 0 (e.g. .dockerenv) is a dotfile, not an ext.
  function fileExt(name: string): string {
    const dot = name.lastIndexOf(".");
    return dot > 0 ? name.slice(dot + 1) : "";
  }
  async function removeEntry(f: FileEntry) {
    const baseDir = browsePath === "/" ? "" : browsePath.replace(/\/$/, "");
    const target = `${baseDir}/${f.name}`;
    if (!confirm(`¿Borrar "${f.name}" dentro del contenedor? No se puede deshacer.`)) return;
    try {
      await api.containerDeletePath(container.id, target);
      notify("success", "Eliminado");
      loadBrowse(browsePath);
    } catch (e) {
      notify("error", String(e));
    }
  }
  async function uploadHere() {
    try {
      const sel = await open({ title: "Selecciona un archivo para subir al contenedor" });
      if (typeof sel !== "string") return;
      uploading = true;
      await api.containerUpload(container.id, browsePath, sel);
      notify("success", "Archivo subido");
      loadBrowse(browsePath);
    } catch (e) {
      notify("error", String(e));
    }
    uploading = false;
  }

  function stopAll() {
    logUnlisten?.();
    execUnlisten?.();
    execExitUnlisten?.();
    if (statsTimer) clearInterval(statsTimer);
    api.logsStop(container.id).catch(() => {});
    if (session) api.execKill(session).catch(() => {});
  }
</script>

<Modal title={container.name} {onClose}>
  <div class="btn-row detail-tabs">
    <button class="btn {tab === 'logs' ? 'primary' : ''}" onclick={() => setTab("logs")}>Logs</button>
    <button class="btn {tab === 'terminal' ? 'primary' : ''}" onclick={() => setTab("terminal")}>Terminal</button>
    <button class="btn {tab === 'stats' ? 'primary' : ''}" onclick={() => setTab("stats")}>Recursos</button>
    <button class="btn {tab === 'files' ? 'primary' : ''}" onclick={() => setTab("files")}>Archivos</button>
    <button class="btn {tab === 'inspect' ? 'primary' : ''}" onclick={() => setTab("inspect")}>Inspeccionar</button>
  </div>

  {#if tab === "logs"}
    <div style="height:46vh"><LogConsole lines={logLines} placeholder="Esperando logs…" /></div>
  {:else if tab === "terminal"}
    <div style="height:40vh"><LogConsole lines={termLines} placeholder="Iniciando shell…" /></div>
    <form onsubmit={sendInput} style="margin-top:10px;display:flex;gap:8px">
      <input type="text" class="mono" placeholder="Comando + Enter…" bind:value={termInput} />
      <button class="btn" type="submit" disabled={!session}>Enviar</button>
    </form>
  {:else if tab === "stats"}
    {#if stats}
      <div class="grid stats-grid">
        <div class="card">
          <div class="label"><span class="card-ic"><Icon name="cpu" size={14} /></span> CPU</div>
          <div class="value">{stats.cpu_percent.toFixed(1)}<small>%</small></div>
          <Sparkline data={cpuHist} max={100} peakLabel="100%" spanLabel="~100 s" />
        </div>
        <div class="card">
          <div class="label"><span class="card-ic"><Icon name="ram" size={14} /></span> Memoria</div>
          <div class="value" style="font-size:19px">
            {bytes(stats.mem_usage)}<small> / {stats.mem_limit ? bytes(stats.mem_limit) : "∞"}</small>
          </div>
          <Sparkline
            data={memHist}
            max={100}
            color="var(--accent-2)"
            peakLabel={stats.mem_limit ? bytes(stats.mem_limit) : "100%"}
            spanLabel="~100 s"
          />
        </div>
        <div class="card">
          <div class="label">Red I/O</div>
          <div class="value" style="font-size:17px">{bytes(netRate)}<small>/s</small></div>
          <div style="color:var(--faint);font-size:11.5px;margin-top:2px">
            ↓ {bytes(stats.net_rx)} · ↑ {bytes(stats.net_tx)} totales
          </div>
          <Sparkline data={netHist} color="var(--ok)" peakLabel={bytes(netPeak) + "/s"} spanLabel="~100 s" />
        </div>
        <div class="card">
          <div class="label">Disco I/O</div>
          <div class="value" style="font-size:17px">{bytes(diskRate)}<small>/s</small></div>
          <div style="color:var(--faint);font-size:11.5px;margin-top:2px">
            {bytes(stats.blk_read)} lect · {bytes(stats.blk_write)} escr
          </div>
          <Sparkline data={diskHist} color="var(--warn)" peakLabel={bytes(diskPeak) + "/s"} spanLabel="~100 s" />
        </div>
      </div>
    {:else}
      <div class="empty">Midiendo…</div>
    {/if}

    <div class="card" style="margin-top:14px">
      <div class="label" style="margin-bottom:10px">Límites de recursos</div>
      <div style="display:flex;gap:10px;flex-wrap:wrap;align-items:flex-end">
        <div class="field" style="margin:0">
          <label for="mem">Memoria máx. (MB)</label>
          <input id="mem" type="text" inputmode="numeric" placeholder="p. ej. 512" bind:value={memLimit} style="width:140px" />
        </div>
        <div class="field" style="margin:0">
          <label for="cpu">CPU máx. (cores)</label>
          <input id="cpu" type="text" inputmode="decimal" placeholder="p. ej. 1.5" bind:value={cpuLimit} style="width:140px" />
        </div>
        <button class="btn primary" onclick={applyLimits} disabled={applyingLimit}>
          {#if applyingLimit}<span class="spinner"></span>{/if} Aplicar
        </button>
      </div>
      <div style="color:var(--faint);font-size:12px;margin-top:8px">
        Evita que un contenedor con fuga de memoria se coma toda la RAM (se reinicia/mata al llegar al tope). Usa 0 para quitar el límite.
      </div>
    </div>
  {:else if tab === "files"}
    <div style="display:flex;align-items:center;gap:8px;margin-bottom:10px">
      <button class="btn" onclick={goUp} disabled={browsePath === "/"}>↑ Arriba</button>
      <span class="mono" style="color:var(--muted);user-select:text;flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis">{browsePath}</span>
      {#if browseLoading}<span class="spinner"></span>{/if}
      <button class="btn primary" onclick={uploadHere} disabled={uploading}>
        {#if uploading}<span class="spinner"></span>{/if} Subir archivo
      </button>
    </div>
    <div class="file-list" style="max-height:44vh">
      {#if browseEntries.length === 0 && !browseLoading}
        <div style="padding:14px;color:var(--faint)">Carpeta vacía o sin acceso.</div>
      {/if}
      {#each browseEntries as f (f.name)}
        <div class="file-row {f.is_dir ? '' : 'file'}">
          {#if f.is_dir}
            <button class="file-main" onclick={() => enterDir(f.name)} title={f.name}>
              <span class="file-ic">📁</span>
              <span class="file-name dir">{f.name}</span>
            </button>
          {:else}
            <div class="file-main" title={f.name}>
              <span class="file-ic">📄</span>
              <span class="file-name">{f.name}</span>
              {#if fileExt(f.name)}<span class="file-ext">{fileExt(f.name)}</span>{/if}
            </div>
          {/if}
          <button class="file-del" title="Borrar del contenedor" aria-label="Borrar" onclick={() => removeEntry(f)}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13" /></svg>
          </button>
        </div>
      {/each}
    </div>
  {:else}
    <pre class="json" style="max-height:58vh">{inspectJson || "Cargando…"}</pre>
  {/if}
</Modal>
