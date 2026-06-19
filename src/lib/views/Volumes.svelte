<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { guard, notify, copyText, askConfirm } from "../stores";
  import type { Volume } from "../types";
  import Modal from "../components/Modal.svelte";
  import { wslPath } from "../format";
  import Icon from "../components/Icon.svelte";

  let items = $state<Volume[]>([]);
  let loading = $state(false);
  let showCreate = $state(false);
  let name = $state("");
  let inspectJson = $state<string | null>(null);

  // ── multi-select delete ──
  let sel = $state<Record<string, boolean>>({});
  let selectedNames = $derived(items.filter((v) => sel[v.name]).map((v) => v.name));
  let allSelected = $derived(items.length > 0 && items.every((v) => sel[v.name]));
  function toggleSel(name: string) {
    sel = { ...sel, [name]: !sel[name] };
  }
  function toggleAll() {
    if (allSelected) {
      sel = {};
    } else {
      const next: Record<string, boolean> = {};
      for (const v of items) next[v.name] = true;
      sel = next;
    }
  }
  async function bulkRemove() {
    const names = selectedNames;
    if (names.length === 0) return;
    if (
      !(await askConfirm({
        message: `¿Eliminar ${names.length} ${names.length === 1 ? "volumen" : "volúmenes"} seleccionados? Se perderán sus datos.`,
      }))
    )
      return;
    let ok = 0;
    let fail = 0;
    for (const n of names) {
      try {
        await api.removeVolume(n, true);
        ok++;
      } catch {
        fail++;
      }
    }
    sel = {};
    notify(fail ? "error" : "success", fail ? `${ok} eliminados, ${fail} con error` : `${ok} volúmenes eliminados`);
    load();
  }

  async function load() {
    loading = true;
    try {
      items = await api.listVolumes();
    } catch (e) {
      notify("error", String(e));
    }
    loading = false;
  }
  onMount(load);

  async function create() {
    if (!name.trim()) return;
    if (await guard(() => api.createVolume(name.trim()), "Volumen creado")) {
      showCreate = false;
      name = "";
      load();
    }
  }
  async function remove(v: Volume) {
    if (!(await askConfirm({ message: `¿Eliminar el volumen "${v.name}"? Se perderán sus datos.` }))) return;
    if (await guard(() => api.removeVolume(v.name, true), "Volumen eliminado")) load();
  }
  async function prune() {
    if (!(await askConfirm({ message: "¿Eliminar todos los volúmenes sin usar?" }))) return;
    if (await guard(() => api.pruneVolumes(), "Volúmenes sin usar eliminados")) load();
  }
  async function inspect(v: Volume) {
    try {
      inspectJson = JSON.stringify(await api.inspectVolume(v.name), null, 2);
    } catch (e) {
      notify("error", String(e));
    }
  }

  async function openVol(v: Volume) {
    try {
      await api.openPath(wslPath(v.mountpoint));
    } catch (e) {
      notify("error", String(e));
    }
  }
</script>

<div class="page-head">
  <span class="ph-icon"><Icon name="volume" /></span>
  <h2>Volúmenes</h2>
  <span class="count">{items.length}</span>
  <div class="grow"></div>
  {#if selectedNames.length}
    <button class="btn danger" onclick={bulkRemove}>Eliminar seleccionados ({selectedNames.length})</button>
  {/if}
  <button class="btn primary" onclick={() => (showCreate = true)}>Crear volumen</button>
  <button class="btn" onclick={prune}>Limpiar sin usar</button>
  <button class="btn" onclick={load} disabled={loading}>
    {#if loading}<span class="spinner"></span>{/if} Actualizar
  </button>
</div>

{#if items.length === 0}
  <div class="empty"><div class="big">Sin volúmenes</div>Crea uno para persistir datos.</div>
{:else}
  <div class="table-wrap">
    <table>
      <thead>
        <tr><th style="width:36px;text-align:center"><input type="checkbox" checked={allSelected} onchange={toggleAll} style="width:auto;margin:0;cursor:pointer" aria-label="Seleccionar todo" /></th><th>Nombre</th><th>Driver</th><th>Ubicación (Windows)</th><th></th></tr>
      </thead>
      <tbody>
        {#each items as v (v.name)}
          <tr>
            <td style="text-align:center"><input type="checkbox" checked={!!sel[v.name]} onchange={() => toggleSel(v.name)} style="width:auto;margin:0;cursor:pointer" aria-label="Seleccionar" /></td>
            <td>
              <button class="hash-copy" title="Copiar nombre"
                onclick={() => copyText(v.name, "Nombre del volumen copiado")}><b>{v.name}</b></button>
            </td>
            <td>{v.driver}</td>
            <td class="mono">
              <div style="user-select:text">{wslPath(v.mountpoint)}</div>
              <div style="color:var(--faint);font-size:11px;user-select:text">{v.mountpoint}</div>
            </td>
            <td>
              <div class="cell-actions">
                <button class="btn" title="Abrir en el Explorador de Windows" onclick={() => openVol(v)}>Abrir carpeta</button>
                <button class="btn" onclick={() => inspect(v)}>Inspeccionar</button>
                <button class="btn icon danger" title="Eliminar" aria-label="Eliminar" onclick={() => remove(v)}>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13" /></svg>
                </button>
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

{#if showCreate}
  <Modal title="Crear volumen" onClose={() => (showCreate = false)}>
    <div class="field">
      <label for="vn">Nombre</label>
      <input id="vn" type="text" placeholder="p. ej. mi-datos" bind:value={name} />
    </div>
    {#snippet footer()}
      <button class="btn" onclick={() => (showCreate = false)}>Cancelar</button>
      <button class="btn primary" onclick={create} disabled={!name.trim()}>Crear</button>
    {/snippet}
  </Modal>
{/if}

{#if inspectJson !== null}
  <Modal title="Inspeccionar volumen" onClose={() => (inspectJson = null)}>
    <pre class="json">{inspectJson}</pre>
  </Modal>
{/if}
