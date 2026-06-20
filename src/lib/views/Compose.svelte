<script module lang="ts">
  // Launched projects persist across navigation AND app restarts (localStorage),
  // so Down / Logs keep working after you leave the view. Live status itself is
  // always derived from Docker, so it's accurate even after an external change.
  interface ComposeHist {
    project: string;
    file: string;
    at: number;
  }
  const KEY = "litedock.compose.history";
  function loadHist(): ComposeHist[] {
    try {
      const v = JSON.parse(localStorage.getItem(KEY) || "[]");
      return Array.isArray(v) ? v : [];
    } catch {
      return [];
    }
  }
  function saveHist(h: ComposeHist[]) {
    try {
      localStorage.setItem(KEY, JSON.stringify(h));
    } catch {
      /* ignore */
    }
  }
  // Keep the last run's output + heading so leaving/returning doesn't lose it.
  let lastLines: string[] = [];
  let lastLabel = "";
</script>

<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount, onDestroy } from "svelte";
  import { api, listen, type UnlistenFn } from "../api";
  import { notify, askConfirm } from "../stores";
  import LogConsole from "../components/LogConsole.svelte";
  import Icon from "../components/Icon.svelte";
  import type { Container, OutputLine } from "../types";
  import { ago } from "../format";

  type Proj = {
    name: string;
    file: string;
    at: number;
    total: number;
    running: number;
    live: boolean;
  };

  let file = $state("");
  let project = $state("");
  let busy = $state(""); // current action id, "" = idle
  let lines = $state<string[]>(lastLines);
  let label = $state(lastLabel);
  let history = $state<ComposeHist[]>(loadHist());
  let containers = $state<Container[]>([]);
  let unlisten: UnlistenFn | undefined;
  let poll: ReturnType<typeof setInterval> | undefined;

  // Union of remembered projects + live containers grouped by compose project.
  let projects = $derived.by<Proj[]>(() => {
    const map = new Map<string, Container[]>();
    for (const c of containers) {
      const p = c.compose_project;
      if (!p) continue;
      let arr = map.get(p);
      if (!arr) {
        arr = [];
        map.set(p, arr);
      }
      arr.push(c);
    }
    const names = new Set<string>([...history.map((h) => h.project), ...map.keys()]);
    return [...names]
      .map((name) => {
        const cs = map.get(name) ?? [];
        const hist = history.find((h) => h.project === name);
        return {
          name,
          file: hist?.file ?? "",
          at: hist?.at ?? 0,
          total: cs.length,
          running: cs.filter((c) => c.state === "running").length,
          live: cs.length > 0,
        };
      })
      .sort((a, b) => b.at - a.at || a.name.localeCompare(b.name));
  });

  function rememberLaunch(p: string, f: string) {
    history = [{ project: p, file: f, at: Date.now() }, ...history.filter((h) => h.project !== p)].slice(0, 50);
    saveHist(history);
  }
  function forget(name: string) {
    history = history.filter((h) => h.project !== name);
    saveHist(history);
  }

  async function refresh() {
    try {
      containers = await api.listContainers();
    } catch {
      /* ignore */
    }
  }

  async function ensureListener() {
    if (unlisten) return;
    unlisten = await listen<OutputLine>("compose-output", (e) => {
      const p = e.payload;
      if (!p.line.startsWith("__EXIT__")) {
        lines = [...lines, p.line].slice(-4000);
        lastLines = lines;
      }
    });
  }

  function startLog(heading: string, seed: string[] = []) {
    label = heading;
    lastLabel = heading;
    lines = seed;
    lastLines = seed;
  }

  function guessProject(p: string): string {
    const parts = p.replace(/\\/g, "/").split("/");
    return (parts[parts.length - 2] || "litedock").toLowerCase().replace(/[^a-z0-9_-]/g, "");
  }

  async function pick() {
    const r = await open({
      title: "Selecciona docker-compose.yml",
      filters: [{ name: "Compose", extensions: ["yml", "yaml"] }],
    });
    if (typeof r === "string") {
      file = r;
      if (!project) project = guessProject(r);
    }
  }

  // Run a streamed compose action (up/down/logs) and refresh state.
  async function stream(id: string, heading: string, fn: () => Promise<number>, after?: () => void) {
    busy = id;
    await ensureListener();
    startLog(heading);
    try {
      const code = await fn();
      if (code === 0) {
        notify("success", `${heading} ✓`);
        after?.();
      } else {
        notify("error", `${heading} terminó con código ${code}`);
      }
    } catch (e) {
      notify("error", String(e));
    }
    busy = "";
    refresh();
  }

  async function launch() {
    if (!file) return notify("error", "Selecciona un archivo compose");
    const proj = (project || guessProject(file)).trim();
    await stream(`up:${proj}`, `compose up · ${proj}`, () => api.composeUp(file, proj), () => rememberLaunch(proj, file));
  }
  async function projUp(p: Proj) {
    if (!p.file) return notify("error", "No recuerdo su archivo; vuelve a lanzarlo desde arriba.");
    await stream(`up:${p.name}`, `compose up · ${p.name}`, () => api.composeUp(p.file, p.name), () => rememberLaunch(p.name, p.file));
  }
  async function projDown(p: Proj) {
    if (!(await askConfirm({ title: "Compose down", message: `¿Detener y eliminar el proyecto "${p.name}"? Se borrarán sus contenedores y redes.` }))) return;
    if (p.file) {
      await stream(`down:${p.name}`, `compose down · ${p.name}`, () => api.composeDown(p.file, p.name));
      return;
    }
    // No remembered file → tear down by label (stop+remove + drop default network).
    busy = `down:${p.name}`;
    startLog(`down · ${p.name}`, [`$ eliminando contenedores de "${p.name}"…`]);
    try {
      for (const c of containers.filter((c) => c.compose_project === p.name)) {
        try {
          await api.removeContainer(c.id, true);
          lines = [...lines, `  removido ${c.name}`];
        } catch (e) {
          lines = [...lines, `  error ${c.name}: ${e}`];
        }
      }
      try {
        await api.removeNetwork(`${p.name}_default`);
      } catch {
        /* network may not exist */
      }
      lines = [...lines, "listo."];
      lastLines = lines;
      notify("success", `${p.name} detenido`);
    } catch (e) {
      notify("error", String(e));
    }
    busy = "";
    refresh();
  }
  async function projLogs(p: Proj) {
    if (!p.file) return notify("error", "No recuerdo su archivo; vuelve a lanzarlo desde arriba para ver sus logs.");
    await stream(`logs:${p.name}`, `compose logs · ${p.name}`, () => api.composeLogs(p.file, p.name));
  }

  onMount(() => {
    refresh();
    poll = setInterval(refresh, 4000);
  });
  onDestroy(() => {
    unlisten?.();
    if (poll) clearInterval(poll);
  });
</script>

<div class="page-head">
  <span class="ph-icon"><Icon name="container" /></span>
  <h2>Docker Compose</h2>
  <span class="count">{projects.length}</span>
  <div class="grow"></div>
  <button class="btn" onclick={refresh}>Refrescar</button>
</div>

<div class="card" style="margin-bottom:14px">
  <div class="label" style="margin-bottom:12px">Lanzar un proyecto</div>
  <div class="field">
    <label for="cf">Archivo compose</label>
    <div style="display:flex;gap:8px">
      <input id="cf" type="text" placeholder="docker-compose.yml" bind:value={file} />
      <button class="btn" onclick={pick}>Elegir…</button>
    </div>
  </div>
  <div class="field" style="margin-bottom:12px">
    <label for="pj">Proyecto (opcional)</label>
    <input id="pj" type="text" placeholder="se deduce de la carpeta del archivo" bind:value={project} />
  </div>
  <button class="btn primary" onclick={launch} disabled={!!busy}>
    {#if busy.startsWith("up:")}<span class="spinner"></span>{/if} Levantar (up -d)
  </button>
</div>

{#if projects.length}
  <div class="table-wrap" style="margin-bottom:14px">
    <table>
      <thead>
        <tr><th>Proyecto</th><th>Estado</th><th>Archivo compose</th><th></th></tr>
      </thead>
      <tbody>
        {#each projects as p (p.name)}
          <tr>
            <td>
              <b>{p.name}</b>
              {#if p.at}<div class="mono" style="color:var(--faint)">lanzado {ago(Math.floor(p.at / 1000))}</div>{/if}
            </td>
            <td>
              {#if p.running > 0}
                <span class="badge running"><span class="b-dot"></span>{p.running}/{p.total} activos</span>
              {:else if p.total > 0}
                <span class="badge exited"><span class="b-dot"></span>0/{p.total} detenidos</span>
              {:else}
                <span class="badge"><span class="b-dot"></span>sin contenedores</span>
              {/if}
            </td>
            <td class="mono ellip" title={p.file}>{p.file || "—"}</td>
            <td>
              <div class="cell-actions">
                <button class="btn" onclick={() => projLogs(p)} disabled={!!busy || !p.file} title={p.file ? "Ver logs" : "Sin archivo recordado"}>Logs</button>
                {#if p.running > 0}
                  <button class="btn danger" onclick={() => projDown(p)} disabled={!!busy}>Down</button>
                {:else}
                  <button class="btn ok" onclick={() => projUp(p)} disabled={!!busy || !p.file} title={p.file ? "Volver a levantar" : "Sin archivo recordado"}>Up</button>
                {/if}
                <button class="btn icon" title={p.live ? "Detenlo antes de quitarlo del histórico" : "Quitar del histórico"} aria-label="Quitar del histórico" onclick={() => forget(p.name)} disabled={p.live}>🗑</button>
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{:else}
  <div class="empty" style="margin-bottom:14px">
    <div class="big">Aún no has lanzado ningún proyecto</div>
    Elige un docker-compose.yml arriba y pulsa “Levantar”.
  </div>
{/if}

{#if label}
  <div class="label" style="display:flex;align-items:center;gap:8px;margin-bottom:6px">
    {label}{#if busy}<span class="spinner"></span>{/if}
  </div>
{/if}
<div style="height:34vh">
  <LogConsole {lines} placeholder="La salida de compose aparecerá aquí…" />
</div>

<style>
  .ellip {
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--faint);
  }
</style>
