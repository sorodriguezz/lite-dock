<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { guard, notify, copyText, route, imagesFilter, askConfirm } from "../stores";
  import type { Container, Stats } from "../types";
  import { shortId, bytes } from "../format";
  import Icon from "../components/Icon.svelte";
  import ContainerDetails from "./ContainerDetails.svelte";
  import RunContainer from "./RunContainer.svelte";

  let items = $state<Container[]>([]);
  let loading = $state(false);
  let q = $state("");
  let selected = $state<Container | null>(null);
  let liveStats = $state<Record<string, Stats>>({});
  let statsTimer: ReturnType<typeof setInterval> | undefined;
  let collapsed = $state<Record<string, boolean>>({});
  let showRun = $state(false);

  async function load() {
    loading = true;
    try {
      items = await api.listContainers();
    } catch (e) {
      notify("error", String(e));
    }
    loading = false;
  }

  // Silent refresh for the live poll (no spinner) so states update in real time.
  async function refresh() {
    try {
      items = await api.listContainers();
    } catch {
      /* ignore transient errors */
    }
  }

  // Left-accent colour for a container row, by state.
  function stateColor(s: string): string {
    if (s === "running") return "var(--ok)";
    if (s === "exited" || s === "dead") return "var(--danger)";
    if (s === "paused" || s === "created" || s === "restarting") return "var(--warn)";
    return "var(--faint)";
  }

  async function pollStats() {
    const running = items.filter((c) => c.state === "running");
    await Promise.allSettled(
      running.map(async (c) => {
        try {
          liveStats[c.id] = await api.containerStats(c.id);
          liveStats = { ...liveStats };
        } catch {
          /* container may have stopped */
        }
      }),
    );
  }

  onMount(() => {
    load().then(pollStats);
    // Poll the list + stats so state changes (running → exited, etc.) show live.
    statsTimer = setInterval(async () => {
      await refresh();
      await pollStats();
    }, 2500);
    return () => clearInterval(statsTimer);
  });

  let filtered = $derived(
    items.filter((c) => !q || c.name.includes(q) || c.image.includes(q)),
  );

  // Group containers launched by the same compose project (like Docker Desktop).
  let grouped = $derived.by(() => {
    const map = new Map<string, Container[]>();
    const standalone: Container[] = [];
    for (const c of filtered) {
      if (c.compose_project) {
        const arr = map.get(c.compose_project) ?? [];
        arr.push(c);
        map.set(c.compose_project, arr);
      } else {
        standalone.push(c);
      }
    }
    const groups = [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
    return { groups, standalone };
  });

  function toggleGroup(name: string) {
    collapsed = { ...collapsed, [name]: !collapsed[name] };
  }
  function openImage(c: Container) {
    imagesFilter.set(c.image);
    route.set("images");
  }
  async function act(fn: () => Promise<unknown>, msg: string) {
    if (await guard(fn, msg)) load();
  }
  // Like `act`, but asks for confirmation first (restart / stop / remove).
  async function confirmAct(message: string, fn: () => Promise<unknown>, msg: string) {
    if (!(await askConfirm({ message }))) return;
    await act(fn, msg);
  }
  function ports(c: Container): string {
    return c.ports
      .filter((p) => p.public_port)
      .map((p) => `${p.public_port}:${p.private_port}/${p.type}`)
      .join(", ");
  }

  // ── multi-select delete ──
  let sel = $state<Record<string, boolean>>({});
  let selectedIds = $derived(filtered.filter((c) => sel[c.id]).map((c) => c.id));
  let allSelected = $derived(filtered.length > 0 && filtered.every((c) => sel[c.id]));
  function toggleSel(id: string) {
    sel = { ...sel, [id]: !sel[id] };
  }
  function toggleAll() {
    if (allSelected) {
      sel = {};
    } else {
      const next: Record<string, boolean> = {};
      for (const c of filtered) next[c.id] = true;
      sel = next;
    }
  }
  async function bulkRemove() {
    const ids = selectedIds;
    if (ids.length === 0) return;
    if (
      !(await askConfirm({
        message: `¿Eliminar ${ids.length} ${ids.length === 1 ? "contenedor" : "contenedores"} seleccionados? No se puede deshacer.`,
      }))
    )
      return;
    let ok = 0;
    let fail = 0;
    for (const id of ids) {
      try {
        await api.removeContainer(id, true);
        ok++;
      } catch {
        fail++;
      }
    }
    sel = {};
    notify(fail ? "error" : "success", fail ? `${ok} eliminados, ${fail} con error` : `${ok} contenedores eliminados`);
    load();
  }
</script>

<div class="page-head">
  <span class="ph-icon"><Icon name="container" /></span>
  <h2>Contenedores</h2>
  <span class="count">{filtered.length}</span>
  <div class="grow"></div>
  {#if selectedIds.length}
    <button class="btn danger" onclick={bulkRemove}>Eliminar seleccionados ({selectedIds.length})</button>
  {/if}
  <button class="btn primary" onclick={() => (showRun = true)}>Ejecutar contenedor</button>
  <input class="search" type="search" placeholder="Buscar nombre o imagen…" bind:value={q} />
  <button class="btn" onclick={load} disabled={loading}>
    {#if loading}<span class="spinner"></span>{/if} Actualizar
  </button>
</div>

{#if filtered.length === 0}
  <div class="empty">
    <div class="big">Sin contenedores</div>
    Descarga una imagen, o usa Build / Compose para crear contenedores.
  </div>
{:else}
  <div class="table-wrap">
    <table>
      <thead>
        <tr>
          <th style="width:36px;text-align:center"><input type="checkbox" checked={allSelected} onchange={toggleAll} style="width:auto;margin:0;cursor:pointer" aria-label="Seleccionar todo" /></th>
          <th>Estado</th>
          <th>Nombre</th>
          <th>Imagen</th>
          <th>Puertos</th>
          <th>CPU</th>
          <th>RAM</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each grouped.groups as [proj, list] (proj)}
          <tr class="group-row" onclick={() => toggleGroup(proj)}>
            <td colspan="8">
              <div class="group-head">
                <span class="group-chev {collapsed[proj] ? '' : 'open'}">▸</span>
                <span style="color:var(--accent);display:inline-flex"><Icon name="layers" size={15} /></span>
                <b>{proj}</b>
                <span class="group-count">
                  {list.length} {list.length === 1 ? "servicio" : "servicios"} · compose
                </span>
              </div>
            </td>
          </tr>
          {#if !collapsed[proj]}
            {#each list as c (c.id)}
              {@render row(c, true)}
            {/each}
          {/if}
        {/each}
        {#each grouped.standalone as c (c.id)}
          {@render row(c, false)}
        {/each}
      </tbody>
    </table>
  </div>
{/if}

{#snippet row(c: Container, inGroup: boolean)}
  <tr onclick={() => (selected = c)} style="cursor:pointer">
    <td style="text-align:center;box-shadow:inset 4px 0 0 {stateColor(c.state)}" onclick={(e) => e.stopPropagation()}>
      <input type="checkbox" checked={!!sel[c.id]} onchange={() => toggleSel(c.id)} style="width:auto;margin:0;cursor:pointer" aria-label="Seleccionar" />
    </td>
    <td class={inGroup ? "indent" : ""}>
      <span style="color:{stateColor(c.state)};display:inline-flex;vertical-align:-3px;margin-right:8px" title={c.state}><Icon name="container" size={15} /></span>
      <span class="badge {c.state}"><span class="b-dot"></span>{c.state}</span>
    </td>
    <td>
      <b>{c.name}</b>
      <button
        class="hash-copy mono"
        style="color:var(--faint)"
        title="Copiar ID completo"
        onclick={(e) => {
          e.stopPropagation();
          copyText(c.id, "ID del contenedor copiado");
        }}
      >{shortId(c.id)}</button>
    </td>
    <td>
      <button
        class="hash-copy mono"
        title="Ver la imagen descargada"
        onclick={(e) => {
          e.stopPropagation();
          openImage(c);
        }}
      >{c.image}</button>
    </td>
    <td class="mono">{ports(c) || "—"}</td>
    <td class="mono">{liveStats[c.id] ? liveStats[c.id].cpu_percent.toFixed(1) + "%" : "—"}</td>
    <td class="mono">{liveStats[c.id] ? bytes(liveStats[c.id].mem_usage) : "—"}</td>
    <td onclick={(e) => e.stopPropagation()}>
      <div class="cell-actions">
        {#if c.state === "running"}
          <button class="btn icon" title="Reiniciar" aria-label="Reiniciar" onclick={() => confirmAct(`¿Reiniciar el contenedor "${c.name}"?`, () => api.restartContainer(c.id), "Reiniciado")}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-2.64-6.36" /><path d="M21 4v4h-4" /></svg>
          </button>
          <button class="btn icon" title="Pausar" aria-label="Pausar" onclick={() => act(() => api.pauseContainer(c.id), "Pausado")}>
            <svg viewBox="0 0 24 24" fill="currentColor"><rect x="8" y="5" width="3.2" height="14" rx="1" /><rect x="12.8" y="5" width="3.2" height="14" rx="1" /></svg>
          </button>
          <button class="btn icon" title="Detener" aria-label="Detener" onclick={() => confirmAct(`¿Detener el contenedor "${c.name}"?`, () => api.stopContainer(c.id), "Detenido")}>
            <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2" /></svg>
          </button>
        {:else if c.state === "paused"}
          <button class="btn icon ok" title="Reanudar" aria-label="Reanudar" onclick={() => act(() => api.unpauseContainer(c.id), "Reanudado")}>
            <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="7 4 20 12 7 20" /></svg>
          </button>
        {:else}
          <button class="btn icon ok" title="Iniciar" aria-label="Iniciar" onclick={() => act(() => api.startContainer(c.id), "Iniciado")}>
            <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="7 4 20 12 7 20" /></svg>
          </button>
        {/if}
        <button class="btn icon danger" title="Eliminar" aria-label="Eliminar" onclick={() => confirmAct(`¿Eliminar el contenedor "${c.name}"? No se puede deshacer.`, () => api.removeContainer(c.id, true), "Eliminado")}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13" /></svg>
        </button>
      </div>
    </td>
  </tr>
{/snippet}

{#if selected}
  <ContainerDetails
    container={selected}
    onClose={() => {
      selected = null;
      load();
    }}
  />
{/if}

{#if showRun}
  <RunContainer onClose={() => (showRun = false)} onCreated={load} />
{/if}
