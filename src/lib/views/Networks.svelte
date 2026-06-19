<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { guard, notify, askConfirm } from "../stores";
  import type { Network } from "../types";
  import Modal from "../components/Modal.svelte";
  import { shortId } from "../format";
  import Icon from "../components/Icon.svelte";

  let items = $state<Network[]>([]);
  let loading = $state(false);
  let showCreate = $state(false);
  let name = $state("");
  let driver = $state("bridge");
  let inspectJson = $state<string | null>(null);

  async function load() {
    loading = true;
    try {
      items = await api.listNetworks();
    } catch (e) {
      notify("error", String(e));
    }
    loading = false;
  }
  onMount(load);

  async function create() {
    if (!name.trim()) return;
    if (await guard(() => api.createNetwork(name.trim(), driver), "Red creada")) {
      showCreate = false;
      name = "";
      load();
    }
  }
  async function remove(n: Network) {
    if (!(await askConfirm({ message: `¿Eliminar la red "${n.name}"?` }))) return;
    if (await guard(() => api.removeNetwork(n.name), "Red eliminada")) load();
  }
  async function prune() {
    if (!(await askConfirm({ message: "¿Eliminar todas las redes sin usar?" }))) return;
    if (await guard(() => api.pruneNetworks(), "Redes sin usar eliminadas")) load();
  }
  async function inspect(n: Network) {
    try {
      inspectJson = JSON.stringify(await api.inspectNetwork(n.name), null, 2);
    } catch (e) {
      notify("error", String(e));
    }
  }

  const builtin = (n: string) => ["bridge", "host", "none"].includes(n);

  // ── multi-select delete (built-in networks excluded) ──
  let sel = $state<Record<string, boolean>>({});
  let selectable = $derived(items.filter((n) => !builtin(n.name)));
  let selectedNames = $derived(selectable.filter((n) => sel[n.name]).map((n) => n.name));
  let allSelected = $derived(selectable.length > 0 && selectable.every((n) => sel[n.name]));
  function toggleSel(name: string) {
    sel = { ...sel, [name]: !sel[name] };
  }
  function toggleAll() {
    if (allSelected) {
      sel = {};
    } else {
      const next: Record<string, boolean> = {};
      for (const n of selectable) next[n.name] = true;
      sel = next;
    }
  }
  async function bulkRemove() {
    const names = selectedNames;
    if (names.length === 0) return;
    if (
      !(await askConfirm({
        message: `¿Eliminar ${names.length} ${names.length === 1 ? "red" : "redes"} seleccionadas?`,
      }))
    )
      return;
    let ok = 0;
    let fail = 0;
    for (const n of names) {
      try {
        await api.removeNetwork(n);
        ok++;
      } catch {
        fail++;
      }
    }
    sel = {};
    notify(fail ? "error" : "success", fail ? `${ok} eliminadas, ${fail} con error` : `${ok} redes eliminadas`);
    load();
  }
</script>

<div class="page-head">
  <span class="ph-icon"><Icon name="network" /></span>
  <h2>Redes</h2>
  <span class="count">{items.length}</span>
  <div class="grow"></div>
  {#if selectedNames.length}
    <button class="btn danger" onclick={bulkRemove}>Eliminar seleccionadas ({selectedNames.length})</button>
  {/if}
  <button class="btn primary" onclick={() => (showCreate = true)}>Crear red</button>
  <button class="btn" onclick={prune}>Limpiar sin usar</button>
  <button class="btn" onclick={load} disabled={loading}>
    {#if loading}<span class="spinner"></span>{/if} Actualizar
  </button>
</div>

{#if items.length === 0}
  <div class="empty"><div class="big">Sin redes</div></div>
{:else}
  <div class="table-wrap">
    <table>
      <thead>
        <tr><th style="width:36px;text-align:center"><input type="checkbox" checked={allSelected} onchange={toggleAll} style="width:auto;margin:0;cursor:pointer" aria-label="Seleccionar todo" /></th><th>Nombre</th><th>Driver</th><th>Scope</th><th>Contenedores</th><th></th></tr>
      </thead>
      <tbody>
        {#each items as n (n.id)}
          <tr>
            <td style="text-align:center">
              {#if !builtin(n.name)}
                <input type="checkbox" checked={!!sel[n.name]} onchange={() => toggleSel(n.name)} style="width:auto;margin:0;cursor:pointer" aria-label="Seleccionar" />
              {/if}
            </td>
            <td>
              <b>{n.name}</b>
              {#if n.internal}<span class="badge" style="margin-left:6px">interna</span>{/if}
              <div class="mono" style="color:var(--faint)">{shortId(n.id)}</div>
            </td>
            <td>{n.driver}</td>
            <td style="color:var(--muted)">{n.scope}</td>
            <td>{n.containers}</td>
            <td>
              <div class="cell-actions">
                <button class="btn" onclick={() => inspect(n)}>Inspeccionar</button>
                {#if !builtin(n.name)}
                  <button class="btn icon danger" title="Eliminar" onclick={() => remove(n)}>🗑</button>
                {/if}
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

{#if showCreate}
  <Modal title="Crear red" onClose={() => (showCreate = false)}>
    <div class="field">
      <label for="nn">Nombre</label>
      <input id="nn" type="text" placeholder="p. ej. mi-red" bind:value={name} />
    </div>
    <div class="field">
      <label for="nd">Driver</label>
      <select id="nd" bind:value={driver}>
        <option value="bridge">bridge</option>
        <option value="macvlan">macvlan</option>
        <option value="ipvlan">ipvlan</option>
      </select>
    </div>
    {#snippet footer()}
      <button class="btn" onclick={() => (showCreate = false)}>Cancelar</button>
      <button class="btn primary" onclick={create} disabled={!name.trim()}>Crear</button>
    {/snippet}
  </Modal>
{/if}

{#if inspectJson !== null}
  <Modal title="Inspeccionar red" onClose={() => (inspectJson = null)}>
    <pre class="json">{inspectJson}</pre>
  </Modal>
{/if}
