<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { onDestroy } from "svelte";
  import { api, listen, type UnlistenFn } from "../api";
  import { notify, askConfirm } from "../stores";
  import LogConsole from "../components/LogConsole.svelte";
  import type { OutputLine } from "../types";
  import { stripAnsi } from "../format";

  let file = $state("");
  let project = $state("");
  let busy = $state(false);
  let lines = $state<string[]>([]);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let services = $state<any[]>([]);
  let unlisten: UnlistenFn | undefined;

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
  function guessProject(p: string): string {
    const parts = p.replace(/\\/g, "/").split("/");
    return (parts[parts.length - 2] || "litedock")
      .toLowerCase()
      .replace(/[^a-z0-9_-]/g, "");
  }

  async function ensureListener() {
    unlisten?.();
    unlisten = await listen<OutputLine>("compose-output", (e) => {
      const p = e.payload;
      if (!p.line.startsWith("__EXIT__")) {
        lines = [...lines, stripAnsi(p.line)].slice(-4000);
      }
    });
  }

  async function run(kind: "up" | "down" | "logs") {
    if (!file) {
      notify("error", "Selecciona un archivo compose");
      return;
    }
    if (
      kind === "down" &&
      !(await askConfirm({
        title: "Compose down",
        message: `¿Detener y eliminar el proyecto "${project || "compose"}"? Se borrarán sus contenedores y redes.`,
      }))
    )
      return;
    busy = true;
    lines = [];
    await ensureListener();
    try {
      const fn =
        kind === "up" ? api.composeUp : kind === "down" ? api.composeDown : api.composeLogs;
      const code = await fn(file, project);
      if (code === 0) notify("success", `compose ${kind} ✓`);
      else notify("error", `compose ${kind} terminó con código ${code}`);
      await refreshPs();
    } catch (e) {
      notify("error", String(e));
    }
    busy = false;
  }

  async function refreshPs() {
    if (!file) return;
    try {
      const r = await api.composePs(file, project);
      services = Array.isArray(r) ? r : [];
    } catch {
      services = [];
    }
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const field = (s: any, ...keys: string[]) => keys.map((k) => s[k]).find((v) => v != null) ?? "—";

  onDestroy(() => unlisten?.());
</script>

<div class="page-head">
  <h2>Docker Compose</h2>
  <div class="grow"></div>
</div>

<div class="card" style="margin-bottom:14px">
  <div class="field">
    <label for="cf">Archivo compose</label>
    <div style="display:flex;gap:8px">
      <input id="cf" type="text" placeholder="docker-compose.yml" bind:value={file} />
      <button class="btn" onclick={pick}>Elegir…</button>
    </div>
  </div>
  <div class="field" style="margin-bottom:0">
    <label for="pj">Proyecto</label>
    <input id="pj" type="text" placeholder="nombre del proyecto" bind:value={project} />
  </div>
  <div class="btn-row" style="margin-top:14px">
    <button class="btn primary" onclick={() => run("up")} disabled={busy}>
      {#if busy}<span class="spinner"></span>{/if} up -d
    </button>
    <button class="btn danger" onclick={() => run("down")} disabled={busy}>down</button>
    <button class="btn" onclick={() => run("logs")} disabled={busy}>logs</button>
    <button class="btn" onclick={refreshPs} disabled={busy}>Refrescar estado</button>
  </div>
</div>

{#if services.length}
  <div class="table-wrap" style="margin-bottom:14px">
    <table>
      <thead>
        <tr><th>Servicio</th><th>Estado</th><th>Detalle</th></tr>
      </thead>
      <tbody>
        {#each services as s, i (i)}
          <tr>
            <td><b>{field(s, "Service", "service", "Name", "name")}</b></td>
            <td><span class="badge {field(s, 'State', 'state')}"><span class="b-dot"></span>{field(s, "State", "state")}</span></td>
            <td style="color:var(--muted)">{field(s, "Status", "status", "Health", "health")}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<div style="height:40vh">
  <LogConsole {lines} placeholder="La salida de compose aparecerá aquí…" />
</div>
