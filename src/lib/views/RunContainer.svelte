<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { notify } from "../stores";
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
  type VolRow = { id: number; source: string; target: string };
  type EnvRow = { id: number; key: string; value: string };

  let idSeq = 0;
  const uid = () => ++idSeq;

  let images = $state<Image[]>([]);
  let image = $state("");
  let name = $state("");
  let pull = $state(false);
  let publishAll = $state(false);
  let restart = $state("no");
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
  });

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

<Modal title="Ejecutar un contenedor" {onClose}>
  <div class="field">
    <label for="rc-name">Nombre (opcional)</label>
    <input id="rc-name" type="text" placeholder="p. ej. mi-nginx" bind:value={name} />
  </div>

  <div class="rc-title">Imagen</div>
  <div class="field img-combo" bind:this={comboEl} style="margin-bottom:8px">
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
                <div class="si-head">
                  <b>{t}</b>
                  <span class="badge" style="margin-left:auto">local</span>
                </div>
              </button>
              {#if looksLikeHub(imageName(t))}
                <button type="button" class="si-hub" title="Ver en Docker Hub"
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
                <div class="si-head">
                  <b>{r.name}</b>
                  {#if r.official}<span class="si-official">oficial</span>{/if}
                  <span class="si-stars" style="margin-left:auto">★ {r.stars}</span>
                </div>
                {#if r.description}<div class="si-desc">{r.description}</div>{/if}
              </button>
              <button type="button" class="si-hub" title="Ver en Docker Hub"
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
  <label class="rc-toggle">
    <input type="checkbox" bind:checked={pull} />
    Siempre traer la imagen (pull)
  </label>

  <div class="rc-card">
    <div class="rc-card-head">
      <span class="ic"><Icon name="network" size={15} /></span>
      Puertos publicados
    </div>
    <label class="rc-toggle">
      <input type="checkbox" bind:checked={publishAll} />
      Publicar todos los puertos expuestos en puertos aleatorios
    </label>
    {#each portRows as row (row.id)}
      <div class="rc-row">
        <input class="rc-port" type="text" placeholder="host" bind:value={row.host} />
        <span class="rc-sep">→</span>
        <input class="rc-port" type="text" placeholder="contenedor" bind:value={row.container} />
        <select class="rc-proto" bind:value={row.proto}>
          <option value="tcp">TCP</option>
          <option value="udp">UDP</option>
        </select>
        <button class="rc-del" title="Quitar" aria-label="Quitar puerto" onclick={() => (portRows = portRows.filter((p) => p.id !== row.id))}>✕</button>
      </div>
    {/each}
    <button class="rc-add rc-add-full" onclick={() => portRows.push({ id: uid(), host: "", container: "", proto: "tcp" })}>
      + Publicar un puerto
    </button>
  </div>

  <div class="rc-card">
    <div class="rc-card-head">
      <span class="ic"><Icon name="volume" size={15} /></span>
      Volúmenes
    </div>
    {#if volRows.length === 0}
      <div class="rc-hint">Sin volúmenes. Monta una carpeta del host o un volumen con nombre.</div>
    {/if}
    {#each volRows as row (row.id)}
      <div class="rc-row">
        <input class="rc-grow" type="text" placeholder="volumen o ruta del host" bind:value={row.source} />
        <span class="rc-sep">→</span>
        <input class="rc-grow" type="text" placeholder="/ruta/en/contenedor" bind:value={row.target} />
        <button class="rc-del" title="Quitar" aria-label="Quitar volumen" onclick={() => (volRows = volRows.filter((p) => p.id !== row.id))}>✕</button>
      </div>
    {/each}
    <button class="rc-add rc-add-full" onclick={() => volRows.push({ id: uid(), source: "", target: "" })}>
      + Añadir volumen
    </button>
  </div>

  <div class="rc-card">
    <div class="rc-card-head">
      <span class="ic"><Icon name="braces" size={15} /></span>
      Variables de entorno
    </div>
    {#if envRows.length === 0}
      <div class="rc-hint">Sin variables. Define pares CLAVE=valor para el contenedor.</div>
    {/if}
    {#each envRows as row (row.id)}
      <div class="rc-row">
        <input class="rc-grow" type="text" placeholder="CLAVE" bind:value={row.key} />
        <span class="rc-sep">=</span>
        <input class="rc-grow" type="text" placeholder="valor" bind:value={row.value} />
        <button class="rc-del" title="Quitar" aria-label="Quitar variable" onclick={() => (envRows = envRows.filter((p) => p.id !== row.id))}>✕</button>
      </div>
    {/each}
    <button class="rc-add rc-add-full" onclick={() => envRows.push({ id: uid(), key: "", value: "" })}>
      + Añadir variable
    </button>
  </div>

  <div class="field" style="margin-top:16px;margin-bottom:0">
    <label for="rc-restart">Política de reinicio</label>
    <select id="rc-restart" bind:value={restart}>
      <option value="no">No reiniciar</option>
      <option value="always">Siempre</option>
      <option value="unless-stopped">A menos que se detenga</option>
      <option value="on-failure">Si falla</option>
    </select>
  </div>

  {#snippet footer()}
    <button class="btn" onclick={onClose}>Cancelar</button>
    <button class="btn primary" onclick={run} disabled={creating || !image.trim()}>
      {#if creating}<span class="spinner"></span>{/if} Crear y ejecutar
    </button>
  {/snippet}
</Modal>

<style>
  .rc-title {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    margin: 18px 0 8px;
  }
  .rc-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: 13px;
    margin-bottom: 10px;
    cursor: pointer;
  }
  .rc-toggle input {
    width: auto;
    margin: 0;
  }
  .rc-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }
  /* One consistent control height so the row never looks misaligned. */
  .rc-row input,
  .rc-row select {
    height: 34px;
  }
  .rc-port {
    width: 110px;
    flex: none;
  }
  .rc-grow {
    flex: 1;
    min-width: 0;
  }
  .rc-proto {
    width: 86px;
    flex: none;
    /* Replace the native dropdown (which renders at a different height and
       throws the row off) with a custom chevron that matches the inputs. */
    appearance: none;
    -webkit-appearance: none;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%238b95ab' stroke-width='2.4' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 9px center;
    background-size: 12px;
    padding-right: 26px;
  }
  .rc-sep {
    color: var(--faint);
    flex: none;
  }
  .rc-del {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--faint);
    border-radius: 6px;
    cursor: pointer;
    line-height: 1;
  }
  .rc-del:hover {
    color: #e5484d;
    border-color: #e5484d;
  }
  .rc-add {
    background: transparent;
    border: 1px dashed var(--border);
    color: var(--text);
    border-radius: 6px;
    padding: 7px 12px;
    font-size: 13px;
    cursor: pointer;
  }
  .rc-add:hover {
    border-color: var(--text);
  }
  .rc-add-full {
    width: 100%;
    text-align: center;
  }

  /* ── Section cards ── */
  .rc-card {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--panel-2);
    padding: 13px 14px;
    margin: 12px 0;
  }
  .rc-card-head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    margin-bottom: 12px;
  }
  .rc-card-head .ic {
    color: var(--accent);
    display: inline-flex;
    align-items: center;
  }
  .rc-hint {
    color: var(--faint);
    font-size: 12.5px;
    margin: 0 0 10px;
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
