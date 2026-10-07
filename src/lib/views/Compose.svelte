<script module lang="ts">
  // Launched projects persist across navigation AND app restarts (localStorage),
  // so Down / Logs keep working after you leave the view. Live status itself is
  // always derived from Docker, so it's accurate even after an external change.
  // The history is module-level state (not per mount) so a `compose up` that
  // finishes while you're on another view is remembered in the list you return
  // to. The running job (busy flag, log, event listener) is in ../jobs.svelte.ts.
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
  let history = $state<ComposeHist[]>(loadHist());
</script>

<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount, untrack } from "svelte";
  import { api } from "../api";
  import { notify, askConfirm } from "../stores";
  import { composeJob as job } from "../jobs.svelte";
  import LogConsole from "../components/LogConsole.svelte";
  import Icon from "../components/Icon.svelte";
  import type { Container } from "../types";
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
  let containers = $state<Container[]>([]);
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

  // Run a streamed compose action (up/down/logs) as the shared compose job; the
  // view refreshes when it ends (see the $effect below).
  async function stream(
    id: string,
    heading: string,
    fn: () => Promise<number>,
    msg: { ok: string; fail: (code: number) => string },
    after?: () => void,
  ) {
    await job.run(id, heading, async () => {
      try {
        const code = await fn();
        if (code === 0) {
          notify("success", msg.ok);
          after?.();
        } else {
          notify("error", msg.fail(code));
        }
      } catch (e) {
        notify("error", String(e));
      }
    });
  }

  function up(name: string, f: string) {
    return stream(
      `up:${name}`,
      `Levantando · ${name}`,
      () => api.composeUp(f, name),
      { ok: `${name} levantado`, fail: (code) => `No se pudo levantar ${name} (código ${code})` },
      () => rememberLaunch(name, f),
    );
  }
  async function launch() {
    if (!file) return notify("error", "Selecciona un archivo compose");
    await up((project || guessProject(file)).trim(), file);
  }
  async function projUp(p: Proj) {
    if (!p.file) return notify("error", "No recuerdo su archivo; vuelve a lanzarlo desde arriba.");
    await up(p.name, p.file);
  }
  async function projDown(p: Proj) {
    if (!(await askConfirm({ title: "Bajar el proyecto", message: `¿Detener y eliminar el proyecto "${p.name}"? Se borrarán sus contenedores y redes.` }))) return;
    if (p.file) {
      await stream(`down:${p.name}`, `Bajando · ${p.name}`, () => api.composeDown(p.file, p.name), {
        ok: `${p.name} detenido`,
        fail: (code) => `No se pudo bajar ${p.name} (código ${code})`,
      });
      return;
    }
    // No remembered file → tear down by label (stop+remove + drop default network).
    const doomed = containers.filter((c) => c.compose_project === p.name);
    await job.run(`down:${p.name}`, `Bajando · ${p.name}`, async () => {
      for (const c of doomed) {
        try {
          await api.removeContainer(c.id, true);
          job.append(`  eliminado ${c.name}`);
        } catch (e) {
          job.append(`  error ${c.name}: ${e}`);
        }
      }
      try {
        await api.removeNetwork(`${p.name}_default`);
      } catch {
        /* network may not exist */
      }
      job.append("listo.");
      notify("success", `${p.name} detenido`);
    }, [`$ eliminando contenedores de "${p.name}"…`]);
  }
  async function projLogs(p: Proj) {
    if (!p.file) return notify("error", "No recuerdo su archivo; vuelve a lanzarlo desde arriba para ver sus logs.");
    await stream(`logs:${p.name}`, `Logs · ${p.name}`, () => api.composeLogs(p.file, p.name), {
      ok: `Logs de ${p.name} obtenidos`,
      fail: (code) => `No se pudieron obtener los logs de ${p.name} (código ${code})`,
    });
  }

  // Refresh on mount and whenever a compose action ends — including one started
  // before you left this view and came back.
  $effect(() => {
    void job.finished;
    untrack(refresh);
  });
  onMount(() => {
    poll = setInterval(refresh, 4000);
    return () => clearInterval(poll);
  });
</script>

<div class="page-head">
  <span class="ph-icon"><Icon name="container" /></span>
  <h2>Docker Compose</h2>
  <p class="page-sub">Proyectos de varios contenedores</p>
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
  <button class="btn primary" onclick={launch} disabled={!!job.busy}>
    {#if job.busy.startsWith("up:")}<span class="spinner"></span>{/if} Levantar
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
                <button class="btn" onclick={() => projLogs(p)} disabled={!!job.busy || !p.file} title={p.file ? "Ver logs" : "Sin archivo recordado"}>Logs</button>
                {#if p.running < p.total || p.total === 0}
                  <button class="btn ok" onclick={() => projUp(p)} disabled={!!job.busy || !p.file} title={p.file ? "Volver a levantar (compose up)" : "Sin archivo recordado"}>Levantar</button>
                {/if}
                <!-- Also offered for stopped projects: otherwise one started from the CLI
                     (no remembered file) could never be removed. -->
                {#if p.total > 0}
                  <button class="btn danger" onclick={() => projDown(p)} disabled={!!job.busy} title="Detener y eliminar sus contenedores y redes (compose down)">Bajar</button>
                {/if}
                <button class="btn icon" title={p.live ? "Detenlo antes de quitarlo del histórico" : "Quitar del histórico"} aria-label="Quitar del histórico" onclick={() => forget(p.name)} disabled={p.live}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13" /></svg></button>
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

{#if job.label}
  <div class="label" style="display:flex;align-items:center;gap:8px;margin-bottom:6px">
    {job.label}{#if job.busy}<span class="spinner"></span>{/if}
  </div>
{/if}
<div style="height:34vh">
  <LogConsole lines={job.lines} placeholder="La salida de compose aparecerá aquí…" />
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
