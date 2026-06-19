<script lang="ts">
  import { onMount } from "svelte";
  import { api, listen, type UnlistenFn } from "../api";
  import { guard, notify, copyText, imagesFilter, askConfirm } from "../stores";
  import { get } from "svelte/store";
  import Icon from "../components/Icon.svelte";
  import type { Image, SearchResult } from "../types";
  import Modal from "../components/Modal.svelte";
  import LogConsole from "../components/LogConsole.svelte";
  import { bytes, ago, shortId, primaryTag } from "../format";

  let items = $state<Image[]>([]);
  let loading = $state(false);
  let q = $state("");

  // pull / search modal
  let showPull = $state(false);
  let searchTerm = $state("");
  let searching = $state(false);
  let results = $state<SearchResult[]>([]);
  let pullRef = $state("");
  let pulling = $state(false);
  let pullDone = $state(false);
  let pullLines = $state<string[]>([]);
  let u1: UnlistenFn | undefined;
  let u2: UnlistenFn | undefined;
  let u3: UnlistenFn | undefined;

  let historyJson = $state<string | null>(null);

  async function load() {
    loading = true;
    try {
      items = await api.listImages();
    } catch (e) {
      notify("error", String(e));
    }
    loading = false;
  }
  onMount(() => {
    // If we arrived via a container's image link, pre-fill the filter.
    const f = get(imagesFilter);
    if (f) {
      q = f;
      imagesFilter.set("");
    }
    load();
  });

  let filtered = $derived(items.filter((i) => !q || primaryTag(i.tags).includes(q)));

  // ── multi-select delete ──
  let sel = $state<Record<string, boolean>>({});
  let selectedIds = $derived(filtered.filter((i) => sel[i.id]).map((i) => i.id));
  let allSelected = $derived(filtered.length > 0 && filtered.every((i) => sel[i.id]));
  function toggleSel(id: string) {
    sel = { ...sel, [id]: !sel[id] };
  }
  function toggleAll() {
    if (allSelected) {
      sel = {};
    } else {
      const next: Record<string, boolean> = {};
      for (const i of filtered) next[i.id] = true;
      sel = next;
    }
  }
  async function bulkRemove() {
    const ids = selectedIds;
    if (ids.length === 0) return;
    if (
      !(await askConfirm({
        message: `¿Eliminar ${ids.length} ${ids.length === 1 ? "imagen" : "imágenes"} seleccionadas? No se puede deshacer.`,
      }))
    )
      return;
    let ok = 0;
    let fail = 0;
    for (const id of ids) {
      try {
        await api.removeImage(id, true);
        ok++;
      } catch {
        fail++;
      }
    }
    sel = {};
    notify(fail ? "error" : "success", fail ? `${ok} eliminadas, ${fail} con error` : `${ok} imágenes eliminadas`);
    load();
  }

  async function openPull() {
    showPull = true;
    pullLines = [];
    pullRef = "";
    pullDone = false;
    pulling = false;
    results = [];
    searchTerm = "";
    u1 = await listen<{ status?: string; id?: string; progress?: string }>(
      "image-pull",
      (e) => {
        const p = e.payload;
        const line =
          (p.status ?? "") + (p.id ? ` ${p.id}` : "") + (p.progress ? ` ${p.progress}` : "");
        if (line.trim()) pullLines = [...pullLines, line].slice(-500);
      },
    );
    u2 = await listen<string>("image-pull-done", () => {
      pulling = false;
      pullDone = true;
      notify("success", "Imagen descargada");
      load();
    });
    u3 = await listen<string>("image-pull-error", (e) => {
      pulling = false;
      notify("error", String(e.payload));
    });
  }
  function closePull() {
    showPull = false;
    u1?.();
    u2?.();
    u3?.();
  }

  async function search() {
    if (!searchTerm.trim()) return;
    searching = true;
    results = [];
    try {
      results = await api.searchImages(searchTerm.trim(), 50);
      if (results.length === 0) notify("info", "Sin resultados");
    } catch (e) {
      notify("error", String(e));
    }
    searching = false;
  }

  function selectResult(r: SearchResult) {
    pullRef = r.name + ":latest";
  }

  // Official images live at hub.docker.com/_/<name>; user/org images at /r/<name>.
  function hubUrl(r: SearchResult): string {
    return r.official
      ? `https://hub.docker.com/_/${r.name}`
      : `https://hub.docker.com/r/${r.name}`;
  }
  async function openHub(r: SearchResult) {
    try {
      await api.openUrl(hubUrl(r));
    } catch (e) {
      notify("error", String(e));
    }
  }

  async function doPull() {
    if (!pullRef.trim()) return;
    pulling = true;
    pullDone = false;
    pullLines = [];
    const [image, tag] = splitRef(pullRef.trim());
    try {
      await api.pullImage(image, tag);
    } catch {
      /* the error event handles messaging */
    }
  }
  function splitRef(ref: string): [string, string] {
    const lastColon = ref.lastIndexOf(":");
    const lastSlash = ref.lastIndexOf("/");
    if (lastColon > lastSlash) return [ref.slice(0, lastColon), ref.slice(lastColon + 1)];
    return [ref, "latest"];
  }

  async function remove(i: Image) {
    if (!(await askConfirm({ message: `¿Eliminar la imagen "${primaryTag(i.tags)}"? No se puede deshacer.` })))
      return;
    if (await guard(() => api.removeImage(i.id, true), "Imagen eliminada")) load();
  }
  async function prune() {
    if (!(await askConfirm({ message: "¿Eliminar todas las imágenes huérfanas (dangling)?" }))) return;
    if (await guard(() => api.pruneImages(), "Imágenes huérfanas eliminadas")) load();
  }
  async function showHistory(i: Image) {
    try {
      historyJson = JSON.stringify(await api.imageHistory(i.id), null, 2);
    } catch (e) {
      notify("error", String(e));
    }
  }
</script>

<div class="page-head">
  <span class="ph-icon"><Icon name="image" /></span>
  <h2>Imágenes</h2>
  <span class="count">{filtered.length}</span>
  <div class="grow"></div>
  {#if selectedIds.length}
    <button class="btn danger" onclick={bulkRemove}>Eliminar seleccionadas ({selectedIds.length})</button>
  {/if}
  <input class="search" type="search" placeholder="Filtrar por tag…" bind:value={q} />
  <button class="btn primary" onclick={openPull}>Buscar / Descargar</button>
  <button class="btn" onclick={prune}>Limpiar huérfanas</button>
  <button class="btn" onclick={load} disabled={loading}>
    {#if loading}<span class="spinner"></span>{/if} Actualizar
  </button>
</div>

{#if filtered.length === 0}
  <div class="empty"><div class="big">Sin imágenes</div>Usa “Buscar / Descargar”.</div>
{:else}
  <div class="table-wrap">
    <table>
      <thead>
        <tr><th style="width:36px;text-align:center"><input type="checkbox" checked={allSelected} onchange={toggleAll} style="width:auto;margin:0;cursor:pointer" aria-label="Seleccionar todo" /></th><th>Tag</th><th>ID</th><th>Tamaño</th><th>Creada</th><th></th></tr>
      </thead>
      <tbody>
        {#each filtered as i (i.id)}
          <tr>
            <td style="text-align:center"><input type="checkbox" checked={!!sel[i.id]} onchange={() => toggleSel(i.id)} style="width:auto;margin:0;cursor:pointer" aria-label="Seleccionar" /></td>
            <td>
              <b>{primaryTag(i.tags)}</b>
              {#if i.dangling}<span class="badge" style="margin-left:6px">dangling</span>{/if}
            </td>
            <td>
              <button class="hash-copy mono" title="Copiar ID completo"
                onclick={() => copyText(i.id, "ID de la imagen copiado")}>{shortId(i.id)}</button>
            </td>
            <td>{bytes(i.size)}</td>
            <td style="color:var(--muted)">{ago(i.created)}</td>
            <td>
              <div class="cell-actions">
                <button class="btn" onclick={() => showHistory(i)}>Historial</button>
                <button class="btn icon danger" title="Eliminar" onclick={() => remove(i)}>🗑</button>
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

{#if showPull}
  <Modal title="Buscar y descargar imágenes" onClose={closePull}>
    <div style="display:flex;gap:8px;margin-bottom:10px">
      <input
        type="search"
        placeholder="Buscar en Docker Hub (p. ej. postgres, nginx, redis)…"
        bind:value={searchTerm}
        onkeydown={(e) => e.key === "Enter" && search()}
      />
      <button class="btn" onclick={search} disabled={searching}>
        {#if searching}<span class="spinner"></span>{/if} Buscar
      </button>
    </div>

    {#if results.length}
      <div class="search-list">
        {#each results as r (r.name)}
          <div class="search-item {pullRef.startsWith(r.name + ':') ? 'sel' : ''}">
            <button class="si-select" onclick={() => selectResult(r)}>
              <div class="si-head">
                <b>{r.name}</b>
                {#if r.official}<span class="si-official">oficial</span>{/if}
                <span class="grow"></span>
                <span class="si-stars">★ {r.stars}</span>
              </div>
              {#if r.description}<div class="si-desc">{r.description}</div>{/if}
            </button>
            <button
              class="si-hub"
              title="Ver en Docker Hub"
              onclick={() => openHub(r)}
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                stroke-linecap="round" stroke-linejoin="round">
                <path d="M7 17 17 7M8 7h9v9" />
              </svg>
              Hub
            </button>
          </div>
        {/each}
      </div>
    {/if}

    <div class="field" style="margin-top:12px;margin-bottom:6px">
      <label for="pref">Imagen a descargar</label>
      <input id="pref" type="text" placeholder="p. ej. postgres:16" bind:value={pullRef} />
    </div>

    {#if pullLines.length || pulling}
      <div style="height:24vh"><LogConsole lines={pullLines} placeholder="Descargando…" /></div>
    {/if}
    {#if pullDone}
      <p style="color:var(--ok);margin:8px 0 0">✓ Imagen descargada y disponible en tu lista.</p>
    {/if}

    {#snippet footer()}
      <button class="btn" onclick={closePull}>Cerrar</button>
      <button class="btn primary" onclick={doPull} disabled={pulling || !pullRef.trim()}>
        {#if pulling}<span class="spinner"></span> Descargando…{:else if pullDone}Descargar de nuevo{:else}Descargar{/if}
      </button>
    {/snippet}
  </Modal>
{/if}

{#if historyJson !== null}
  <Modal title="Historial de capas" onClose={() => (historyJson = null)}>
    <pre class="json">{historyJson}</pre>
  </Modal>
{/if}
