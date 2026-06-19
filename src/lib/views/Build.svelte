<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { onDestroy } from "svelte";
  import { api, listen, type UnlistenFn } from "../api";
  import { notify } from "../stores";
  import LogConsole from "../components/LogConsole.svelte";
  import type { OutputLine } from "../types";
  import { stripAnsi } from "../format";

  let context = $state("");
  let dockerfile = $state("");
  let tag = $state("");
  let building = $state(false);
  let lines = $state<string[]>([]);
  let unlisten: UnlistenFn | undefined;

  async function pickContext() {
    const r = await open({ directory: true, title: "Selecciona el contexto de build" });
    if (typeof r === "string") {
      context = r;
      if (!dockerfile) dockerfile = r + "\\Dockerfile";
    }
  }
  async function pickDockerfile() {
    const r = await open({ title: "Selecciona el Dockerfile" });
    if (typeof r === "string") dockerfile = r;
  }

  async function build() {
    if (!context || !dockerfile) {
      notify("error", "Selecciona el contexto y el Dockerfile");
      return;
    }
    building = true;
    lines = [];
    unlisten?.();
    unlisten = await listen<OutputLine>("build-output", (e) => {
      const p = e.payload;
      if (!p.line.startsWith("__EXIT__")) {
        lines = [...lines, stripAnsi(p.line)].slice(-4000);
      }
    });
    try {
      const code = await api.buildImage(context, dockerfile, tag);
      if (code === 0) notify("success", "Build completado");
      else notify("error", `Build terminó con código ${code}`);
    } catch (e) {
      notify("error", String(e));
    }
    building = false;
  }

  onDestroy(() => unlisten?.());
</script>

<div class="page-head">
  <h2>Build · Dockerfile</h2>
  <div class="grow"></div>
</div>

<div class="card" style="margin-bottom:14px">
  <div class="field">
    <label for="ctx">Contexto de build</label>
    <div style="display:flex;gap:8px">
      <input id="ctx" type="text" placeholder="Carpeta del proyecto" bind:value={context} />
      <button class="btn" onclick={pickContext}>Elegir…</button>
    </div>
  </div>
  <div class="field">
    <label for="df">Dockerfile</label>
    <div style="display:flex;gap:8px">
      <input id="df" type="text" placeholder="Ruta al Dockerfile" bind:value={dockerfile} />
      <button class="btn" onclick={pickDockerfile}>Elegir…</button>
    </div>
  </div>
  <div class="field" style="margin-bottom:0">
    <label for="tg">Tag (opcional)</label>
    <input id="tg" type="text" placeholder="p. ej. miapp:latest" bind:value={tag} />
  </div>
  <div class="btn-row" style="margin-top:14px">
    <button class="btn primary" onclick={build} disabled={building}>
      {#if building}<span class="spinner"></span>{/if} Construir
    </button>
    <span style="color:var(--faint);align-self:center;font-size:12px">
      Usa BuildKit dentro del motor. El contexto se monta vía WSL.
    </span>
  </div>
</div>

<div style="height:48vh">
  <LogConsole {lines} placeholder="La salida del build aparecerá aquí…" />
</div>
