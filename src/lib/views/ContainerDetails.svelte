<script lang="ts">
  import { onMount, tick } from "svelte";
  import LogConsole from "../components/LogConsole.svelte";
  import XTerm from "../components/XTerm.svelte";
  import { api, listen, type UnlistenFn } from "../api";
  import { notify, askConfirm, guard, copyText } from "../stores";
  import type { Container, FileEntry, Stats } from "../types";
  import { bytes, shortId, containerState, stateSince, publishedPorts } from "../format";
  import { makeZip, parseTar, b64ToBytes, type ArchiveFile } from "../archive";
  import { open } from "@tauri-apps/plugin-dialog";
  import Sparkline from "../components/Sparkline.svelte";
  import Icon from "../components/Icon.svelte";

  interface Props {
    container: Container;
    /** Back to the containers list. */
    onBack: () => void;
  }
  let { container, onBack }: Props = $props();

  let st = $derived(containerState(container));
  let ports = $derived(publishedPorts(container.ports));
  let running = $derived(container.state === "running");
  let webPort = $derived(running ? ports.find((p) => p.web) : undefined);

  type Tab = "logs" | "terminal" | "stats" | "files" | "inspect";
  let tab = $state<Tab>("logs");

  // `docker inspect` result: feeds the header (network/IP), the limits form,
  // the Inspeccionar summary and its raw JSON view.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let info = $state<any>(null);
  let showJson = $state(false);
  let inspectJson = $derived(info ? JSON.stringify(info, null, 2) : "");

  let logLines = $state<string[]>([]);
  let logUnlisten: UnlistenFn | undefined;

  type XTermApi = { write: (s: string) => void; clear: () => void; reset: () => void; focus: () => void };
  let session = $state<string | null>(null);
  let xterm = $state<XTermApi>();
  let termPending = $state<string[]>([]);
  let execUnlisten: UnlistenFn | undefined;
  let execExitUnlisten: UnlistenFn | undefined;
  let termStarting = false;
  // Once opened, the terminal stays mounted (hidden) so output keeps rendering
  // while another tab is shown.
  let termMounted = $state(false);
  // Set when the modal closes, so async setup that resolves later cleans itself up.
  let destroyed = false;

  const STATS_MS = 1500; // sample interval for the live graphs
  const STATS_S = STATS_MS / 1000;
  let stats = $state<Stats | null>(null);
  const HIST = 40; // samples kept for the live graphs
  let statsTimer: ReturnType<typeof setTimeout> | undefined;
  let prevAt: number | null = null;
  let firstAt = $state(0); // timestamp of the oldest sample still on the graph
  let histTimes: number[] = [];
  let cpuHist = $state<number[]>([]);
  let memHist = $state<number[]>([]); // bytes used → the graph auto-scales to real usage
  let netHist = $state<number[]>([]);
  let diskHist = $state<number[]>([]);
  let prevNet: number | null = null;
  let prevDisk: number | null = null;
  // Peak memory, for the graph axis label when the container has no memory limit.
  let memPeak = $derived(memHist.length ? Math.max(...memHist) : 0);
  // net/disk history already holds bytes per second.
  let netRate = $derived(netHist.length ? netHist[netHist.length - 1] : 0);
  let diskRate = $derived(diskHist.length ? diskHist[diskHist.length - 1] : 0);
  let netPeak = $derived(netHist.length ? Math.max(...netHist) : 0);
  let diskPeak = $derived(diskHist.length ? Math.max(...diskHist) : 0);
  // CPU can exceed 100 % (one core = 100 %): scale the graph to the real peak.
  let cpuMax = $derived(Math.max(100, ...cpuHist));
  // Real time window covered by the graphs (samples take ~1-2 s each).
  let prevAtView = $state(0);
  let spanLabel = $derived.by(() => {
    const secs = firstAt && prevAtView ? Math.round((prevAtView - firstAt) / 1000) : 0;
    return secs >= 90 ? `~${Math.round(secs / 60)} min` : `~${secs} s`;
  });

  // resource limits
  let memLimit = $state("");
  let cpuLimit = $state("");
  let applyingLimit = $state(false);

  // file browser
  let browsePath = $state("/");
  let browseEntries = $state<FileEntry[]>([]);
  let browseLoading = $state(false);
  let uploading = $state(false);
  let sel = $state<Record<string, boolean>>({});
  let downloading = $state(false);
  let selectedEntries = $derived(browseEntries.filter((f) => sel[f.name]));

  onMount(() => {
    startLogs();
    loadInspect(true);
    return () => stopAll();
  });

  async function loadInspect(prefillLimits = false) {
    try {
      info = await api.inspectContainer(container.id);
      // Pre-fill the limit fields with the container's current limits (persisted by Docker).
      if (prefillLimits) {
        const mem = info?.HostConfig?.Memory ?? 0;
        const nano = info?.HostConfig?.NanoCpus ?? 0;
        if (mem > 0) memLimit = String(Math.round(mem / (1024 * 1024)));
        if (nano > 0) cpuLimit = String(+(nano / 1e9).toFixed(2)).replace(".", ",");
      }
    } catch {
      /* ignore */
    }
  }

  // Networks + IPs, e.g. "tienda_default · 172.19.0.3".
  let networks = $derived.by(() => {
    const nets = info?.NetworkSettings?.Networks ?? {};
    return Object.entries(nets).map(([name, n]) => {
      const ip = (n as { IPAddress?: string })?.IPAddress;
      return ip ? `${name} · ${ip}` : name;
    });
  });

  const RESTART: Record<string, string> = {
    no: "No reiniciar",
    always: "Siempre",
    "unless-stopped": "Salvo que lo detengas",
    "on-failure": "Solo si falla",
  };
  // Human summary for the Inspeccionar tab (the raw JSON stays one click away).
  let facts = $derived.by(() => {
    if (!info) return [];
    const cfg = info.Config ?? {};
    const host = info.HostConfig ?? {};
    const cmd = [...(cfg.Entrypoint ?? []), ...(cfg.Cmd ?? [])].join(" ");
    const mounts = (info.Mounts ?? []).map(
      (m: { Source?: string; Name?: string; Destination?: string; Type?: string }) =>
        `${m.Type === "volume" ? m.Name : m.Source} → ${m.Destination} (${m.Type === "volume" ? "volumen" : m.Type})`,
    );
    const envNames = (cfg.Env ?? []).map((e: string) => e.split("=")[0]);
    const policy = host.RestartPolicy?.Name || "no";
    const out: { k: string; v: string }[] = [
      { k: "Comando", v: cmd || "—" },
      { k: "Directorio de trabajo", v: cfg.WorkingDir || "/" },
      { k: "Creado", v: info.Created ? new Date(info.Created).toLocaleString("es") : "—" },
      { k: "Política de reinicio", v: RESTART[policy] ?? policy },
      { k: "Redes", v: networks.join(", ") || "—" },
      { k: "Montajes", v: mounts.join("\n") || "—" },
      { k: "Variables de entorno", v: envNames.join(", ") || "—" },
    ];
    if (container.compose_project) {
      const svc = cfg.Labels?.["com.docker.compose.service"];
      out.push({ k: "Compose", v: svc ? `${container.compose_project} · servicio ${svc}` : container.compose_project });
    }
    return out;
  });

  // ── header actions ──
  async function headerAct(fn: () => Promise<unknown>, ok: string) {
    if (await guard(fn, ok)) loadInspect();
  }
  async function stopIt() {
    if (await askConfirm({ title: `¿Detener "${container.name}"?`, message: "El contenedor se detendrá; podrás volver a iniciarlo.", confirmText: "Detener" }))
      headerAct(() => api.stopContainer(container.id), `${container.name} detenido`);
  }
  async function restartIt() {
    if (await askConfirm({ title: `¿Reiniciar "${container.name}"?`, message: "Se detendrá y volverá a arrancar.", confirmText: "Reiniciar", danger: false }))
      headerAct(() => api.restartContainer(container.id), `${container.name} reiniciado`);
  }
  async function removeIt() {
    if (!(await askConfirm({ title: `¿Eliminar "${container.name}"?`, message: "Se borrará el contenedor (sus volúmenes con nombre se conservan). No se puede deshacer.", confirmText: "Eliminar" })))
      return;
    if (await guard(() => api.removeContainer(container.id, true), `${container.name} eliminado`)) onBack();
  }
  function openPort(port: number) {
    api.openUrl(`http://localhost:${port}`).catch((e) => notify("error", String(e)));
  }

  async function setTab(t: Tab) {
    tab = t;
    if (t === "inspect") loadInspect();
    if (t === "terminal") {
      if (!session) startTerminal();
      else {
        await tick();
        xterm?.focus();
      }
    }
    if (t === "stats" && !statsTimer) startStats();
    if (t === "files" && browseEntries.length === 0) loadBrowse("/");
  }

  async function startLogs() {
    logLines = [];
    const un = await listen<{ id: string; line: string }>("container-log", (e) => {
      if (e.payload.id !== container.id) return;
      // Each Docker log frame already ends in "\n" and LogConsole joins lines with
      // another one: split the frame and drop the trailing empty piece.
      const chunk = e.payload.line.replace(/\r?\n$/, "").split(/\r?\n/);
      logLines = [...logLines, ...chunk].slice(-2000);
    });
    // The modal may have closed while we were subscribing.
    if (destroyed) return un();
    logUnlisten = un;
    try {
      await api.logsStart(container.id, "300");
      if (destroyed) api.logsStop(container.id).catch(() => {});
    } catch (e) {
      notify("error", String(e));
    }
  }

  async function startTerminal() {
    if (termStarting || session) return;
    termStarting = true;
    termMounted = true;
    // Drop the listeners of a previous (finished) session so output isn't echoed twice.
    execUnlisten?.();
    execExitUnlisten?.();
    execUnlisten = execExitUnlisten = undefined;
    termPending = [];
    xterm?.reset();
    const unOut = await listen<{ session: string; line: string }>("exec-output", (e) => {
      if (e.payload.session === session) termWrite(e.payload.line);
    });
    const unExit = await listen<string>("exec-exit", (e) => {
      if (e.payload === session) {
        termWrite("\r\n\x1b[2m[sesión finalizada — vuelve a abrir la pestaña para otra]\x1b[0m\r\n");
        session = null;
      }
    });
    if (destroyed) {
      unOut();
      unExit();
      termStarting = false;
      return;
    }
    execUnlisten = unOut;
    execExitUnlisten = unExit;
    try {
      const id = await api.execStart(container.id, ["/bin/sh"]);
      if (destroyed) {
        api.execKill(id).catch(() => {});
      } else {
        session = id;
        xterm?.focus();
      }
    } catch (e) {
      notify("error", String(e));
    }
    termStarting = false;
  }

  // xterm forwards every keystroke raw to the shell — Ctrl+C, arrows (the shell's
  // own history), clear, vim/nano, top… all handled natively by the real shell.
  function termWrite(s: string) {
    if (xterm) xterm.write(s);
    else termPending = [...termPending, s];
  }
  function onTermData(d: string) {
    if (session) api.execWrite(session, d).catch((e) => notify("error", String(e)));
  }
  // Flush output that arrived before xterm finished mounting.
  $effect(() => {
    if (xterm && termPending.length) {
      const pending = termPending;
      termPending = [];
      for (const s of pending) xterm.write(s);
    }
  });

  function startStats() {
    // A stats sample itself takes ~1-2 s, so chain timeouts instead of an
    // interval (ticks never overlap) and derive the I/O rates from the real
    // elapsed time between samples.
    const sample = async () => {
      try {
        const s = await api.containerStats(container.id);
        if (destroyed) return;
        const now = performance.now();
        const dt = prevAt === null ? STATS_S : Math.max(0.25, (now - prevAt) / 1000);
        prevAt = now;
        histTimes = [...histTimes, now].slice(-HIST);
        firstAt = histTimes[0];
        prevAtView = now;
        stats = s;
        // Keep rolling windows for the live graphs (net/disk plot bytes per second).
        cpuHist = [...cpuHist, s.cpu_percent].slice(-HIST);
        memHist = [...memHist, s.mem_usage].slice(-HIST);
        const netTotal = s.net_rx + s.net_tx;
        const diskTotal = s.blk_read + s.blk_write;
        if (prevNet !== null) netHist = [...netHist, Math.max(0, netTotal - prevNet) / dt].slice(-HIST);
        if (prevDisk !== null) diskHist = [...diskHist, Math.max(0, diskTotal - prevDisk) / dt].slice(-HIST);
        prevNet = netTotal;
        prevDisk = diskTotal;
      } catch {
        /* container may have stopped */
      }
      if (!destroyed) statsTimer = setTimeout(sample, STATS_MS);
    };
    statsTimer = setTimeout(sample, 0);
  }

  // Parse a limit field: "" → no change; accepts the Spanish decimal comma.
  function parseLimit(raw: string): number | null | "invalid" {
    const t = raw.trim().replace(",", ".");
    if (t === "") return null;
    const n = Number(t);
    return Number.isFinite(n) && n >= 0 ? n : "invalid";
  }

  async function applyLimits() {
    const memRaw = parseLimit(memLimit);
    const cpu = parseLimit(cpuLimit);
    if (memRaw === "invalid" || cpu === "invalid") {
      notify("error", "Escribe solo números en los límites (p. ej. 512 o 1,5).");
      return;
    }
    const mem = memRaw === null ? null : Math.round(memRaw);
    if (mem === null && cpu === null) {
      notify("info", "No hay ningún límite que aplicar.");
      return;
    }
    applyingLimit = true;
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
      sel = {};
    } catch (e) {
      notify("error", String(e));
    }
    browseLoading = false;
  }
  function enterDir(name: string) {
    const base = browsePath === "/" ? "" : browsePath.replace(/\/$/, "");
    loadBrowse(`${base}/${name}`);
  }
  // Clickable path segments for the file browser breadcrumb.
  let pathParts = $derived.by(() => {
    const parts = browsePath.split("/").filter(Boolean);
    return parts.map((name, i) => ({ name, path: "/" + parts.slice(0, i + 1).join("/") }));
  });
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
    if (!(await askConfirm({ message: `¿Borrar "${f.name}" dentro del contenedor? No se puede deshacer.` })))
      return;
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

  // Join a Windows destination dir with a filename.
  function joinHost(dir: string, name: string): string {
    const sep = dir.includes("\\") || !dir.includes("/") ? "\\" : "/";
    return dir.replace(/[\\/]+$/, "") + sep + name;
  }
  async function downloadSel() {
    const items = selectedEntries;
    if (!items.length) return;
    const destDir = await open({ directory: true, title: "Carpeta de destino para la descarga" });
    if (typeof destDir !== "string") return;
    downloading = true;
    try {
      const base = browsePath === "/" ? "" : browsePath.replace(/\/$/, "");
      // A single plain file downloads as-is; a folder (or any multi-select) is zipped.
      if (items.length === 1 && !items[0].is_dir) {
        const n = await api.containerDownloadToHost(container.id, `${base}/${items[0].name}`, joinHost(destDir, items[0].name));
        notify("success", `Descargado: ${items[0].name} (${bytes(n)})`);
      } else {
        const entries: ArchiveFile[] = [];
        for (const it of items) {
          const b64 = await api.containerDownload(container.id, `${base}/${it.name}`, it.is_dir);
          const bytes = b64ToBytes(b64);
          if (it.is_dir) {
            const parsed = parseTar(bytes);
            // An empty folder yields no files — keep it as a directory entry so
            // the download still works instead of erroring out.
            if (parsed.length) entries.push(...parsed);
            else entries.push({ name: it.name + "/", data: new Uint8Array(0), dir: true });
          } else {
            entries.push({ name: it.name, data: bytes });
          }
        }
        const zip = makeZip(entries);
        const zipName =
          (items.length === 1 && items[0].is_dir ? items[0].name : `${container.name}_descarga`) + ".zip";
        await api.writeHostFile(joinHost(destDir, zipName), Array.from(zip));
        const n = entries.filter((e) => !e.dir).length;
        notify("success", `Descargado: ${zipName} (${n} archivo${n === 1 ? "" : "s"})`);
      }
      sel = {};
    } catch (e) {
      notify("error", String(e));
    }
    downloading = false;
  }

  function stopAll() {
    logUnlisten?.();
    execUnlisten?.();
    execExitUnlisten?.();
    destroyed = true;
    if (statsTimer) clearTimeout(statsTimer);
    api.logsStop(container.id).catch(() => {});
    if (session) api.execKill(session).catch(() => {});
  }
</script>

<div class="page-head" style="padding-bottom:0;gap:12px">
  <nav class="crumbs" aria-label="Ruta" style="flex-basis:100%">
    <button onclick={onBack}>Contenedores</button>
    {#if container.compose_project}
      <span class="sep">/</span><span>{container.compose_project}</span>
    {/if}
    <span class="sep">/</span><span class="here">{container.name}</span>
  </nav>

  <h2 class="ellip" style="max-width:min(520px, 60vw)">{container.name}</h2>
  <span class="badge {st.kind}" title={container.status}>
    <span class="b-dot"></span>{st.label}{#if stateSince(container)} · {stateSince(container)}{/if}
  </span>
  <div class="grow"></div>
  {#if webPort}
    <button class="btn" onclick={() => openPort(webPort.port)}><Icon name="external" /> Abrir localhost:{webPort.port}</button>
  {/if}
  {#if running}
    <button class="btn" onclick={restartIt}><Icon name="restart" /> Reiniciar</button>
    <button class="btn" onclick={stopIt}><Icon name="stop" /> Detener</button>
  {:else if container.state === "paused"}
    <button class="btn ok" onclick={() => headerAct(() => api.unpauseContainer(container.id), `${container.name} reanudado`)}><Icon name="play" /> Reanudar</button>
  {:else}
    <button class="btn ok" onclick={() => headerAct(() => api.startContainer(container.id), `${container.name} iniciado`)}><Icon name="play" /> Iniciar</button>
  {/if}
  <button class="btn icon danger" title="Eliminar contenedor" aria-label="Eliminar {container.name}" onclick={removeIt}><Icon name="trash" /></button>

  <dl class="meta" style="flex-basis:100%">
    <div><dt>Imagen</dt><dd class="mono">{container.image}</dd></div>
    <div>
      <dt>ID</dt>
      <dd style="display:flex;align-items:center;gap:4px">
        <span class="mono">{shortId(container.id)}</span>
        <button class="btn icon ghost" style="width:28px;min-height:28px" title="Copiar ID completo" aria-label="Copiar ID completo" onclick={() => copyText(container.id, "ID del contenedor copiado")}><Icon name="copy" /></button>
      </dd>
    </div>
    {#if ports.length}
      <div><dt>Puertos</dt><dd class="mono">{ports.map((p) => p.label).join(", ")}</dd></div>
    {/if}
    {#if networks.length}
      <div><dt>Red</dt><dd>{networks.join(", ")}</dd></div>
    {/if}
  </dl>

  <div class="tabs" role="tablist" aria-label="Secciones del contenedor" style="flex-basis:calc(100% + 64px)">
    <button role="tab" aria-selected={tab === "logs"} onclick={() => setTab("logs")}>Logs</button>
    <button role="tab" aria-selected={tab === "terminal"} onclick={() => setTab("terminal")} disabled={!running} title={running ? "" : "Inicia el contenedor para abrir una terminal"}>Terminal</button>
    <button role="tab" aria-selected={tab === "stats"} onclick={() => setTab("stats")}>Recursos</button>
    <button role="tab" aria-selected={tab === "files"} onclick={() => setTab("files")} disabled={!running} title={running ? "" : "Inicia el contenedor para explorar sus archivos"}>Archivos</button>
    <button role="tab" aria-selected={tab === "inspect"} onclick={() => setTab("inspect")}>Inspeccionar</button>
  </div>
</div>

<div role="tabpanel">
  <!-- The terminal stays mounted once opened (just hidden) so its output keeps
       rendering while another tab is visible. -->
  {#if termMounted}
    <div hidden={tab !== "terminal"}>
      <div style="display:flex;align-items:center;gap:10px;margin-bottom:10px;font-size:13px;color:var(--text-2)">
        <span class="dot {session ? 'up' : ''}" style="box-shadow:none"></span>
        {session ? "Conectado a /bin/sh" : "Sesión finalizada"}
        <div style="flex:1"></div>
        {#if !session && running}
          <button class="btn" onclick={startTerminal}>Nueva sesión</button>
        {/if}
      </div>
      <div style="height:calc(100vh - 380px);min-height:320px">
        <XTerm bind:this={xterm} ondata={onTermData} />
      </div>
      <p style="color:var(--faint);font-size:12.5px;margin:8px 0 0">
        Terminal interactiva: Ctrl+C, historial con ↑/↓, <code>top</code>, <code>vim</code> y <code>nano</code> funcionan como en una consola normal.
      </p>
    </div>
  {/if}

  {#if tab === "logs"}
    <div class="toolbar" style="justify-content:flex-end;margin-bottom:10px">
      <span style="flex:1;color:var(--muted);font-size:12.5px">Últimas {logLines.length} líneas · siguiendo en vivo</span>
      <button class="btn" onclick={() => copyText(logLines.join("\n"), "Logs copiados")} disabled={!logLines.length}><Icon name="copy" /> Copiar</button>
      <button class="btn ghost" onclick={() => (logLines = [])} disabled={!logLines.length}>Limpiar</button>
    </div>
    <div style="height:calc(100vh - 360px);min-height:300px"><LogConsole lines={logLines} placeholder="Esperando logs…" /></div>
  {:else if tab === "terminal"}
    <!-- rendered above -->
  {:else if tab === "stats"}
    {#if stats}
      <div class="grid stats-grid">
        <div class="card">
          <div class="label"><span class="card-ic"><Icon name="cpu" size={14} /></span> CPU</div>
          <div class="value">{stats.cpu_percent.toFixed(1)}<small>%</small></div>
          <Sparkline data={cpuHist} max={cpuMax} peakLabel="{Math.round(cpuMax)}%" {spanLabel} />
        </div>
        <div class="card">
          <div class="label"><span class="card-ic"><Icon name="ram" size={14} /></span> Memoria</div>
          <div class="value" style="font-size:22px">
            {bytes(stats.mem_usage)}<small> / {stats.mem_limit ? bytes(stats.mem_limit) : "∞"}</small>
          </div>
          <Sparkline
            data={memHist}
            max={stats.mem_limit || undefined}
            color="var(--accent-2)"
            peakLabel={stats.mem_limit ? bytes(stats.mem_limit) : bytes(memPeak)}
            {spanLabel}
          />
        </div>
        <div class="card">
          <div class="label"><span class="card-ic"><Icon name="network" size={14} /></span> Red</div>
          <div class="value" style="font-size:22px">{bytes(netRate)}<small>/s</small></div>
          <div class="sub">↓ {bytes(stats.net_rx)} · ↑ {bytes(stats.net_tx)} en total</div>
          <Sparkline data={netHist} color="var(--ok)" peakLabel={bytes(netPeak) + "/s"} {spanLabel} />
        </div>
        <div class="card">
          <div class="label"><span class="card-ic"><Icon name="disk" size={14} /></span> Disco</div>
          <div class="value" style="font-size:22px">{bytes(diskRate)}<small>/s</small></div>
          <div class="sub">{bytes(stats.blk_read)} leídos · {bytes(stats.blk_write)} escritos</div>
          <Sparkline data={diskHist} color="var(--warn)" peakLabel={bytes(diskPeak) + "/s"} {spanLabel} />
        </div>
      </div>
    {:else if !running}
      <div class="empty">El contenedor no está en ejecución: no hay consumo que medir.</div>
    {:else}
      <div class="empty"><span class="spinner"></span>Midiendo…</div>
    {/if}

    <section class="card" style="margin-top:14px" aria-labelledby="lim-h">
      <h3 id="lim-h">Límites de recursos</h3>
      <p style="margin:4px 0 14px;color:var(--muted);font-size:13px">
        Evita que un contenedor con fuga de memoria se coma la RAM del motor (se reinicia o se detiene al llegar al tope). Deja un campo vacío para no cambiarlo.
      </p>
      <div style="display:flex;gap:12px;flex-wrap:wrap;align-items:flex-end">
        <div class="field" style="margin:0">
          <label for="mem">Memoria máxima (MB)</label>
          <input id="mem" type="text" inputmode="numeric" placeholder="sin límite" bind:value={memLimit} style="width:150px" />
        </div>
        <div class="field" style="margin:0">
          <label for="cpu">CPU máxima (núcleos)</label>
          <input id="cpu" type="text" inputmode="decimal" placeholder="sin límite" bind:value={cpuLimit} style="width:150px" />
        </div>
        <button class="btn primary lg" onclick={applyLimits} disabled={applyingLimit}>
          {#if applyingLimit}<span class="spinner"></span>{/if} Aplicar límites
        </button>
      </div>
    </section>
  {:else if tab === "files"}
    <div class="toolbar" style="margin-bottom:10px">
      <nav class="crumbs mono" aria-label="Ruta en el contenedor" style="flex:1 1 260px;min-width:0;font-size:13px">
        <button onclick={() => loadBrowse("/")}>/</button>
        {#each pathParts as part, i (part.path)}
          {#if i < pathParts.length - 1}
            <button onclick={() => loadBrowse(part.path)}>{part.name}</button><span class="sep">/</span>
          {:else}
            <span class="here">{part.name}</span>
          {/if}
        {/each}
        {#if browseLoading}<span class="spinner" style="margin-left:6px"></span>{/if}
      </nav>
      <button class="btn" onclick={goUp} disabled={browsePath === "/"}>Subir un nivel</button>
      {#if selectedEntries.length}
        <button class="btn" onclick={downloadSel} disabled={downloading}>
          {#if downloading}<span class="spinner"></span>{:else}<Icon name="download" />{/if} Descargar ({selectedEntries.length})
        </button>
      {/if}
      <button class="btn primary" onclick={uploadHere} disabled={uploading}>
        {#if uploading}<span class="spinner"></span>{:else}<Icon name="upload" />{/if} Subir archivo
      </button>
    </div>
    <div class="file-list" style="max-height:calc(100vh - 360px)">
      {#if browseEntries.length === 0 && !browseLoading}
        <div style="padding:14px;color:var(--faint)">Carpeta vacía o sin acceso.</div>
      {/if}
      {#each browseEntries as f (f.name)}
        <div class="file-row {f.is_dir ? '' : 'file'}" style={sel[f.name] ? "background:#14231f" : ""}>
          <label style="display:flex;align-items:center;padding:0 2px 0 12px;cursor:pointer">
            <input
              type="checkbox"
              checked={!!sel[f.name]}
              onchange={() => (sel = { ...sel, [f.name]: !sel[f.name] })}
              aria-label="Seleccionar {f.name}"
            />
          </label>
          {#if f.is_dir}
            <button class="file-main" onclick={() => enterDir(f.name)} title={f.name}>
              <span class="file-ic" style="color:var(--accent-2)"><Icon name="folder" size={16} /></span>
              <span class="file-name dir">{f.name}</span>
            </button>
          {:else}
            <div class="file-main" title={f.name}>
              <span class="file-ic" style="color:var(--muted)"><Icon name="file" size={16} /></span>
              <span class="file-name">{f.name}</span>
              {#if fileExt(f.name)}<span class="file-ext">{fileExt(f.name)}</span>{/if}
            </div>
          {/if}
          <button class="file-del" title="Borrar del contenedor" aria-label="Borrar {f.name}" onclick={() => removeEntry(f)}>
            <Icon name="trash" size={15} />
          </button>
        </div>
      {/each}
    </div>
  {:else}
    <div class="toolbar" style="justify-content:flex-end;margin-bottom:10px">
      <button class="btn" onclick={() => (showJson = !showJson)} aria-pressed={showJson}>
        <Icon name="braces" /> {showJson ? "Ver resumen" : "Ver JSON completo"}
      </button>
      <button class="btn" onclick={() => copyText(inspectJson, "JSON copiado")} disabled={!inspectJson}><Icon name="copy" /> Copiar JSON</button>
    </div>
    {#if !info}
      <div class="empty"><span class="spinner"></span>Cargando…</div>
    {:else if showJson}
      <pre class="json" style="max-height:calc(100vh - 340px)">{inspectJson}</pre>
    {:else}
      <dl class="facts">
        {#each facts as f (f.k)}
          <dt>{f.k}</dt>
          <dd style="white-space:pre-line">{f.v}</dd>
        {/each}
      </dl>
    {/if}
  {/if}
</div>
