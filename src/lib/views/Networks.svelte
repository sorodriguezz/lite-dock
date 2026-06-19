<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { guard, notify } from "../stores";
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
    if (await guard(() => api.removeNetwork(n.name), "Red eliminada")) load();
  }
  async function prune() {
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
</script>

<div class="page-head">
  <span class="ph-icon"><Icon name="network" /></span>
  <h2>Redes</h2>
  <span class="count">{items.length}</span>
  <div class="grow"></div>
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
        <tr><th>Nombre</th><th>Driver</th><th>Scope</th><th>Contenedores</th><th></th></tr>
      </thead>
      <tbody>
        {#each items as n (n.id)}
          <tr>
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
