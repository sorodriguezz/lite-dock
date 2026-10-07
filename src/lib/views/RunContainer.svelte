<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { notify, copyText } from "../stores";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import type { Image, SearchResult } from "../types";
  import { primaryTag } from "../format";
  import Modal from "../components/Modal.svelte";
  import Icon from "../components/Icon.svelte";

  interface Props {
    onClose: () => void;
    onCreated: () => void;
  }
  let { onClose, onCreated }: Props = $props();

  type PortRow = { id: number; host: string; container: string; proto: string };
  type VolRow = { id: number; kind: "volume" | "bind"; source: string; target: string };
  type EnvRow = { id: number; key: string; value: string; show?: boolean };

  let idSeq = 0;
  const uid = () => ++idSeq;

  let images = $state<Image[]>([]);
  let image = $state("");
  let name = $state("");
  let pull = $state(false);
  let publishAll = $state(false);
  let restart = $state("no");
  let autoRemove = $state(false);
  // Host ports already published by other containers → their name (conflict warning).
  let usedPorts = $state<Record<string, string>>({});
  let creating = $state(false);

  let portRows = $state<PortRow[]>([{ id: uid(), host: "", container: "", proto: "tcp" }]);
  let volRows = $state<VolRow[]>([]);
  let envRows = $state<EnvRow[]>([]);

  // Tagged images the user can pick from (free text is also allowed).
  let runnable = $derived(
    images.map((i) => primaryTag(i.tags)).filter((t) => t && t !== "<none>:<none>"),
  );

  onMount(async () => {
    try {
      images = await api.listImages();
      if (!image && runnable.length) image = runnable[0];
    } catch (e) {
      notify("error", String(e));
    }
    try {
      const used: Record<string, string> = {};
      for (const c of await api.listContainers()) {
        if (c.state !== "running") continue;
        for (const p of c.ports) if (p.public_port) used[`${p.public_port}/${p.type}`] = c.name;
      }
      usedPorts = used;
    } catch {
      /* conflict hints are best-effort */
    }
  });

  // Host side may be "8080" or "127.0.0.1:8080": the port is the last segment.
  const hostPortOf = (h: string) => h.trim().split(":").pop() ?? "";
  function conflictOf(r: PortRow): string | null {
    const hp = hostPortOf(r.host);
    if (!hp) return null;
    return usedPorts[`${hp}/${r.proto === "udp" ? "udp" : "tcp"}`] ?? null;
  }
  // First free port above the requested one, for the "Usar N" quick fix.
  function freePortFrom(r: PortRow): number {
    let n = Number(hostPortOf(r.host)) + 1;
    const proto = r.proto === "udp" ? "udp" : "tcp";
    const taken = new Set(portRows.filter((x) => x !== r).map((x) => hostPortOf(x.host)));
    while (usedPorts[`${n}/${proto}`] || taken.has(String(n))) n++;
    return n;
  }

  const SECRET_RE = /(pass|pwd|secret|token|key|credential)/i;
  const isSecret = (k: string) => SECRET_RE.test(k);

  async function pickFolder(row: VolRow) {
    const dir = await openDialog({ directory: true, title: "Carpeta de Windows a montar" });
    if (typeof dir === "string") row.source = dir;
  }

  // Paste KEY=VALUE lines (a .env file) from the clipboard.
  async function pasteEnv() {
    try {
      const text = await navigator.clipboard.readText();
      const rows = text
        .split(/\r?\n/)
        .map((l) => l.trim())
        .filter((l) => l && !l.startsWith("#") && l.includes("="))
        .map((l) => {
          const i = l.indexOf("=");
          const value = l.slice(i + 1).trim().replace(/^(['"])(.*)\1$/, "$2");
          return { id: uid(), key: l.slice(0, i).replace(/^export\s+/, "").trim(), value };
        });
      if (!rows.length) return notify("info", "El portapapeles no tiene líneas CLAVE=valor.");
      envRows = [...envRows.filter((r) => r.key.trim()), ...rows];
      notify("success", `${rows.length} variable${rows.length === 1 ? "" : "s"} añadida${rows.length === 1 ? "" : "s"}`);
    } catch {
      notify("error", "No se pudo leer el portapapeles.");
    }
  }

  // Equivalent `docker run` (secrets masked) — handy to copy into scripts.
  const q = (v: string) => (/^[\w@%+=:,./-]+$/.test(v) ? v : `"${v.replace(/"/g, '\\"')}"`);
  let command = $derived.by(() => {
    const parts = ["docker run -d"];
    if (name.trim()) parts.push(`--name ${q(name.trim())}`);
    if (restart !== "no") parts.push(`--restart ${restart}`);
    if (autoRemove) parts.push("--rm");
    if (publishAll) parts.push("-P");
    for (const r of portRows) {
      if (!r.container.trim()) continue;
      const proto = r.proto === "udp" ? "/udp" : "";
      parts.push(`-p ${r.host.trim() ? `${r.host.trim()}:` : ""}${r.container.trim()}${proto}`);
    }
    for (const r of volRows) if (r.source.trim() && r.target.trim()) parts.push(`-v ${q(`${r.source.trim()}:${r.target.trim()}`)}`);
    for (const r of envRows) if (r.key.trim()) parts.push(`-e ${q(`${r.key.trim()}=${isSecret(r.key) && r.value ? "••••••" : r.value}`)}`);
    parts.push(image.trim() || "<imagen>");
    return parts.join(" ");
  });

  function onFormKey(e: KeyboardEvent) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && !creating && image.trim()) {
      e.preventDefault();
      run();
    }
  }

  // ── Image combobox: local images + Docker Hub search ──
  let comboOpen = $state(false);
  let comboEl = $state<HTMLElement>();
  let hubResults = $state<SearchResult[]>([]);
  let hubLoading = $state(false);
  let hubTerm = $state("");
  let searchTimer: ReturnType<typeof setTimeout> | undefined;

  // Show the whole local list when the field is empty or holds an exact pick;
  // otherwise narrow it as the user types.
  let localMatches = $derived.by(() => {
    const q = image.trim().toLowerCase();
    if (!q || runnable.some((t) => t.toLowerCase() === q)) return runnable;
    return runnable.filter((t) => t.toLowerCase().includes(q));
  });

  const imageName = (ref: string) =>
    ref.includes(":") ? ref.slice(0, ref.lastIndexOf(":")) : ref;
  const isOfficial = (name: string) => !name.includes("/");
  // Only link to Docker Hub for plain Hub refs (skip ghcr.io/, localhost:5000/, etc.).
  const looksLikeHub = (name: string) => {
    const host = name.split("/")[0];
    return !host.includes(".") && !host.includes(":");
  };
  const hubUrl = (name: string, official: boolean) =>
    official ? `https://hub.docker.com/_/${name}` : `https://hub.docker.com/r/${name}`;
  async function openHub(name: string, official: boolean) {
    try {
      await api.openUrl(hubUrl(name, official));
    } catch (e) {
      notify("error", String(e));
    }
  }

  function openCombo() {
    comboOpen = true;
    maybeSearch();
  }
  function onImgInput() {
    comboOpen = true;
    maybeSearch();
  }
  function pickLocal(t: string) {
    image = t;
    comboOpen = false;
  }
  function pickHub(r: SearchResult) {
    image = r.name + ":latest";
    comboOpen = false;
  }
  function maybeSearch() {
    clearTimeout(searchTimer);
    const raw = image.trim();
    const term = raw.includes(":") ? raw.slice(0, raw.lastIndexOf(":")) : raw;
    if (term.length < 2) {
      hubResults = [];
      hubLoading = false;
      hubTerm = "";
      return;
    }
    if (term.toLowerCase() === hubTerm.toLowerCase() && hubResults.length) return;
    hubLoading = true;
    searchTimer = setTimeout(async () => {
      try {
        hubTerm = term;
        hubResults = await api.searchImages(term, 8);
      } catch {
        hubResults = [];
      }
      hubLoading = false;
    }, 350);
  }

  // Close the dropdown when clicking outside of it.
  $effect(() => {
    if (!comboOpen) return;
    const onDocPointer = (e: PointerEvent) => {
      if (comboEl && !comboEl.contains(e.target as Node)) comboOpen = false;
    };
    document.addEventListener("pointerdown", onDocPointer, true);
    return () => document.removeEventListener("pointerdown", onDocPointer, true);
  });

  async function run() {
    if (!image.trim()) {
      notify("error", "Elige o escribe una imagen");
      return;
    }
    const ports = portRows
      .filter((r) => r.container.trim())
      .map((r) => {
        const cont = r.container.trim();
        const proto = r.proto === "udp" ? "udp" : "tcp";
        return r.host.trim() ? `${r.host.trim()}:${cont}/${proto}` : `${cont}/${proto}`;
      });
    const volumes = volRows
      .filter((r) => r.source.trim() && r.target.trim())
      .map((r) => `${r.source.trim()}:${r.target.trim()}`);
    const env = envRows.filter((r) => r.key.trim()).map((r) => `${r.key.trim()}=${r.value}`);

    creating = true;
    try {
      await api.runContainer({
        image: image.trim(),
        name: name.trim() || undefined,
        ports,
        env,
        volumes,
        restart,
        pull,
        publishAll,
        autoRemove,
      });
      notify("success", "Contenedor creado y en ejecución");
      onCreated();
      onClose();
    } catch (e) {
      notify("error", String(e));
    }
    creating = false;
  }
</script>

<Modal title="Ejecutar contenedor" {onClose}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div onkeydown={onFormKey}>
  <p class="rc-lead">Equivale a <code>docker run</code>. Solo la imagen es obligatoria.</p>

  <div class="rc-two">
    <div class="field img-combo" bind:this={comboEl} style="margin:0">
      <label for="rc-img">Imagen</label>
    <div class="img-input">
        <input
          id="rc-img"
          type="text"
          autocomplete="off"
          role="combobox"
          aria-expanded={comboOpen}
          aria-controls="rc-img-pop"
          placeholder="p. ej. nginx:latest o busca en Docker Hub…"
          bind:value={image}
          oninput={onImgInput}
          onfocus={openCombo}
          onkeydown={(e) => e.key === "Escape" && (comboOpen = false)}
        />
        <button
          class="img-caret"
          type="button"
          tabindex="-1"
          aria-label={comboOpen ? "Ocultar lista" : "Mostrar lista"}
          onclick={() => (comboOpen ? (comboOpen = false) : openCombo())}
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2"
            stroke-linecap="round" stroke-linejoin="round" class:open={comboOpen}><path d="m6 9 6 6 6-6" /></svg>
        </button>
      </div>

    {#if comboOpen}
        <div class="img-pop" id="rc-img-pop">
          <div class="img-sec">Tus imágenes</div>
          {#if localMatches.length}
            {#each localMatches as t (t)}
              <div class="search-item {image === t ? 'sel' : ''}">
                <button type="button" class="si-select" onclick={() => pickLocal(t)}>
                  <span class="si-head">
                    <b>{t}</b>
                    <span class="badge" style="margin-left:auto">descargada</span>
                  </span>
                </button>
                {#if looksLikeHub(imageName(t))}
                  <button type="button" class="si-hub" title="Ver en Docker Hub" aria-label="Ver {imageName(t)} en Docker Hub"
                    onclick={() => openHub(imageName(t), isOfficial(imageName(t)))}>
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                      stroke-linecap="round" stroke-linejoin="round"><path d="M7 17 17 7M8 7h9v9" /></svg>
                    Hub
                  </button>
                {/if}
              </div>
            {/each}
          {:else}
            <div class="img-empty">Ninguna imagen local coincide.</div>
          {/if}

          <div class="img-sec">
            Docker Hub {#if hubLoading}<span class="spinner"></span>{/if}
          </div>
          {#if hubResults.length}
            {#each hubResults as r (r.name)}
              <div class="search-item {image.startsWith(r.name + ':') ? 'sel' : ''}">
                <button type="button" class="si-select" onclick={() => pickHub(r)}>
                  <span class="si-head">
                    <b>{r.name}</b>
                    {#if r.official}<span class="si-official">oficial</span>{/if}
                    <span class="si-stars" style="margin-left:auto">★ {r.stars}</span>
                  </span>
                  {#if r.description}<span class="si-desc">{r.description}</span>{/if}
                </button>
                <button type="button" class="si-hub" title="Ver en Docker Hub" aria-label="Ver {r.name} en Docker Hub"
                  onclick={() => openHub(r.name, r.official)}>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                    stroke-linecap="round" stroke-linejoin="round"><path d="M7 17 17 7M8 7h9v9" /></svg>
                  Hub
                </button>
              </div>
            {/each}
          {:else if !hubLoading}
            <div class="img-empty">
              {image.trim().length < 2
                ? "Escribe al menos 2 letras para buscar en Docker Hub…"
                : "Sin resultados en Docker Hub."}
            </div>
          {/if}
        </div>
      {/if}
    </div>
    <div class="field" style="margin:0">
      <label for="rc-name">Nombre <span class="rc-opt">(opcional)</span></label>
      <input id="rc-name" type="text" placeholder="p. ej. mi-nginx" bind:value={name} />
    </div>
  </div>

  <fieldset class="rc-set">
    <legend>Puertos</legend>
    {#each portRows as row (row.id)}
      {@const clash = conflictOf(row)}
      <div class="rc-grid ports">
        <label>En Windows<input type="text" inputmode="numeric" placeholder="8080" class:warn={clash} bind:value={row.host} /></label>
        <span class="rc-arrow" aria-hidden="true">→</span>
        <label>En el contenedor<input type="text" inputmode="numeric" placeholder="80" bind:value={row.container} /></label>
        <label>Protocolo<select bind:value={row.proto}><option value="tcp">TCP</option><option value="udp">UDP</option></select></label>
        <button class="btn icon ghost" title="Quitar" aria-label="Quitar puerto" onclick={() => (portRows = portRows.filter((p) => p.id !== row.id))}><Icon name="x" size={15} /></button>
      </div>
      {#if clash}
        <div class="inline-alert" role="alert">
          <Icon name="alert" />
          <span style="flex:1">El puerto {hostPortOf(row.host)} ya lo usa <b>{clash}</b>.</span>
          <button class="btn" style="min-height:30px;font-size:12.5px" onclick={() => (row.host = String(freePortFrom(row)))}>Usar {freePortFrom(row)}</button>
        </div>
      {/if}
    {/each}
    <div class="rc-foot">
      <button class="btn link" onclick={() => portRows.push({ id: uid(), host: "", container: "", proto: "tcp" })}>+ Añadir puerto</button>
      <label class="rc-check"><input type="checkbox" bind:checked={publishAll} /> Publicar todos los puertos expuestos en puertos aleatorios</label>
    </div>
  </fieldset>

  <fieldset class="rc-set">
    <legend>Volúmenes</legend>
    {#if volRows.length === 0}
      <p class="rc-hint">Sin volúmenes. Monta una carpeta de Windows o un volumen con nombre para guardar datos.</p>
    {/if}
    {#each volRows as row (row.id)}
      <div class="rc-grid vols">
        <label>Tipo<select bind:value={row.kind}><option value="volume">Volumen</option><option value="bind">Carpeta de Windows</option></select></label>
        <label>Origen
          <span class="rc-with-btn">
            <input type="text" placeholder={row.kind === "bind" ? "C:\\proyectos\\datos" : "mis-datos"} bind:value={row.source} />
            {#if row.kind === "bind"}
              <button class="btn" title="Elegir carpeta" onclick={() => pickFolder(row)}>Elegir…</button>
            {/if}
          </span>
        </label>
        <span class="rc-arrow" aria-hidden="true">→</span>
        <label>Ruta en el contenedor<input type="text" placeholder="/datos" bind:value={row.target} /></label>
        <button class="btn icon ghost" title="Quitar" aria-label="Quitar volumen" onclick={() => (volRows = volRows.filter((p) => p.id !== row.id))}><Icon name="x" size={15} /></button>
      </div>
    {/each}
    <div class="rc-foot">
      <button class="btn link" onclick={() => volRows.push({ id: uid(), kind: "volume", source: "", target: "" })}>+ Añadir volumen</button>
    </div>
  </fieldset>

  <fieldset class="rc-set">
    <legend>Variables de entorno</legend>
    {#if envRows.length === 0}
      <p class="rc-hint">Sin variables. Define pares CLAVE=valor, o pega un archivo .env.</p>
    {/if}
    {#each envRows as row (row.id)}
      <div class="rc-grid env">
        <input type="text" placeholder="CLAVE" aria-label="Nombre de la variable" class="mono" bind:value={row.key} />
        <span class="rc-with-btn">
          <input
            type={isSecret(row.key) && !row.show ? "password" : "text"}
            placeholder="valor"
            aria-label="Valor de {row.key || 'la variable'}"
            bind:value={row.value}
          />
          {#if isSecret(row.key)}
            <button class="btn icon ghost" aria-label={row.show ? "Ocultar valor" : "Mostrar valor"} aria-pressed={!!row.show} onclick={() => (row.show = !row.show)}><Icon name="eye" /></button>
          {/if}
        </span>
        <button class="btn icon ghost" title="Quitar" aria-label="Quitar variable" onclick={() => (envRows = envRows.filter((p) => p.id !== row.id))}><Icon name="x" size={15} /></button>
      </div>
    {/each}
    <div class="rc-foot">
      <button class="btn link" onclick={() => envRows.push({ id: uid(), key: "", value: "" })}>+ Añadir variable</button>
      <button class="btn link" onclick={pasteEnv}>Pegar desde .env</button>
    </div>
  </fieldset>

  <div class="rc-opts">
    <div class="field" style="margin:0">
      <label for="rc-restart">Si se detiene</label>
      <select id="rc-restart" bind:value={restart} style="width:250px" disabled={autoRemove} title={autoRemove ? "Un contenedor que se borra al detenerse no puede reiniciarse" : ""}>
        <option value="no">No reiniciar</option>
        <option value="unless-stopped">Reiniciar salvo que lo detenga yo</option>
        <option value="always">Reiniciar siempre</option>
        <option value="on-failure">Reiniciar solo si falla</option>
      </select>
    </div>
    <!-- Docker rejects --rm together with a restart policy, so picking it resets the policy. -->
    <label class="rc-check"><input type="checkbox" bind:checked={autoRemove} onchange={() => autoRemove && (restart = "no")} /> Borrar al detenerse</label>
    <label class="rc-check"><input type="checkbox" bind:checked={pull} /> Descargar siempre la última versión</label>
  </div>

  <div class="rc-cmd">
    <div class="rc-cmd-head">
      <span>Comando equivalente</span>
      <button class="btn ghost" style="min-height:28px;font-size:12.5px" onclick={() => copyText(command, "Comando copiado")}><Icon name="copy" /> Copiar</button>
    </div>
    <pre>{command}</pre>
  </div>
  </div>

  {#snippet footer()}
    <span class="rc-kbd">Ctrl + Enter para ejecutar</span>
    <button class="btn lg" onclick={onClose}>Cancelar</button>
    <button class="btn primary lg" onclick={run} disabled={creating || !image.trim()}>
      {#if creating}<span class="spinner"></span>{:else}<Icon name="play" />{/if} Ejecutar
    </button>
  {/snippet}
</Modal>

<style>
  .rc-lead {
    margin: -6px 0 16px;
    color: var(--muted);
    font-size: 13px;
  }
  .rc-opt {
    font-weight: 400;
    color: var(--faint);
  }
  .rc-two {
    display: grid;
    grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr);
    gap: 14px;
    margin-bottom: 16px;
  }
  .rc-set {
    margin: 0 0 14px;
    padding: 12px 16px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .rc-set legend {
    padding: 0 6px;
    font-size: 13px;
    font-weight: 600;
  }
  .rc-grid {
    display: grid;
    gap: 8px;
    align-items: end;
  }
  .rc-grid.ports {
    grid-template-columns: minmax(0, 1fr) 16px minmax(0, 1fr) 96px 36px;
  }
  .rc-grid.vols {
    grid-template-columns: 150px minmax(0, 1.3fr) 16px minmax(0, 1fr) 36px;
  }
  .rc-grid.env {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr) 36px;
    align-items: center;
  }
  .rc-grid label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--muted);
    min-width: 0;
  }
  .rc-grid input.warn {
    border-color: #b45309;
  }
  .rc-arrow {
    height: 38px;
    display: grid;
    place-items: center;
    color: var(--faint);
  }
  .rc-with-btn {
    display: flex;
    gap: 6px;
    min-width: 0;
  }
  .rc-with-btn input {
    flex: 1;
    min-width: 0;
  }
  .rc-foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 14px;
    margin-left: -8px;
  }
  .rc-hint {
    margin: 0;
    color: var(--faint);
    font-size: 12.5px;
  }
  .rc-check {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 38px;
    color: var(--text-2);
    font-size: 13px;
    cursor: pointer;
  }
  .rc-opts {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 6px 20px;
    margin-bottom: 14px;
  }
  .rc-cmd {
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
  }
  .rc-cmd-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 6px 0 14px;
    font-size: 12px;
    color: var(--faint);
  }
  .rc-cmd pre {
    margin: 0;
    padding: 4px 14px 12px;
    font-family: var(--mono);
    font-size: 12.5px;
    line-height: 1.6;
    color: var(--text-2);
    white-space: pre-wrap;
    word-break: break-all;
    user-select: text;
  }
  .rc-kbd {
    flex: 1;
    font-size: 12.5px;
    color: var(--faint);
  }

  /* ── Image combobox ── */
  .img-combo {
    position: relative;
  }
  .img-input {
    position: relative;
    display: flex;
    align-items: center;
  }
  .img-input input {
    padding-right: 36px;
  }
  .img-caret {
    position: absolute;
    right: 6px;
    top: 50%;
    transform: translateY(-50%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    background: transparent;
    border: none;
    color: var(--faint);
    cursor: pointer;
    border-radius: 6px;
  }
  .img-caret:hover {
    color: var(--text);
    background: var(--panel-3);
  }
  .img-caret svg {
    width: 16px;
    height: 16px;
    transition: transform 0.15s ease;
  }
  .img-caret svg.open {
    transform: rotate(180deg);
  }
  .img-pop {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 20;
    background: var(--bg);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow);
    max-height: 320px;
    overflow: auto;
    padding: 4px 0;
  }
  .img-sec {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 10.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--faint);
    padding: 9px 12px 5px;
  }
  .img-sec .spinner {
    width: 12px;
    height: 12px;
  }
  .img-empty {
    color: var(--muted);
    font-size: 12px;
    padding: 2px 12px 10px;
  }
</style>
