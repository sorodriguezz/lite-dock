<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { guard, notify, copyText, route, imagesFilter, askConfirm, openContainerReq } from "../stores";
  import type { Container, Stats } from "../types";
  import { shortId, bytes, containerState, stateSince, publishedPorts } from "../format";
  import Icon from "../components/Icon.svelte";
  import ContainerDetails from "./ContainerDetails.svelte";
  import RunContainer from "./RunContainer.svelte";

  let items = $state<Container[]>([]);
  let loading = $state(false);
  let loaded = $state(false);
  let q = $state("");
  let filter = $state<"all" | "running" | "stopped">("all");
  let liveStats = $state<Record<string, Stats>>({});
  let statsTimer: ReturnType<typeof setInterval> | undefined;
  let collapsed = $state<Record<string, boolean>>({});
  let showRun = $state(false);
  let menuFor = $state<string | null>(null);
  // The menu is position:fixed (the table scrolls and would clip it), anchored to its button.
  let menuPos = $state({ top: 0, right: 0, up: false });
  function toggleMenu(id: string, btn: HTMLElement) {
    if (menuFor === id) {
      menuFor = null;
      return;
    }
    const r = btn.getBoundingClientRect();
    const up = r.bottom + 220 > window.innerHeight;
    menuPos = { top: up ? r.top - 4 : r.bottom + 4, right: window.innerWidth - r.right, up };
    menuFor = id;
  }

  // Detail page: the container is looked up in the live list so its state
  // (running → exited…) updates while the page is open.
  let detailId = $state<string | null>(null);
  let lastDetail: Container | null = null;
  let detail = $derived.by(() => {
    if (!detailId) return null;
    const c = items.find((x) => x.id === detailId) ?? (lastDetail?.id === detailId ? lastDetail : null);
    lastDetail = c;
    return c;
  });

  async function load() {
    loading = true;
    try {
      items = await api.listContainers();
    } catch (e) {
      notify("error", String(e));
    }
    loading = false;
    loaded = true;
  }

  // Silent refresh for the live poll (no spinner) so states update in real time.
  async function refresh() {
    try {
      items = await api.listContainers();
    } catch {
      /* ignore transient errors */
    }
  }

  async function pollStats() {
    const running = items.filter((c) => c.state === "running");
    // Forget samples of containers that stopped, so they don't show stale CPU/RAM.
    const live = new Set(running.map((c) => c.id));
    if (Object.keys(liveStats).some((id) => !live.has(id))) {
      liveStats = Object.fromEntries(Object.entries(liveStats).filter(([id]) => live.has(id)));
    }
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

  // Another view (e.g. the Panel) asked to open a container's detail page.
  $effect(() => {
    const id = $openContainerReq;
    if (id) {
      detailId = id;
      openContainerReq.set(null);
    }
  });

  onMount(() => {
    load().then(pollStats);
    // Poll the list + stats so state changes (running → exited, etc.) show live.
    // A stats sample takes ~1-2 s, so skip a tick while the previous one runs.
    let busy = false;
    statsTimer = setInterval(async () => {
      if (busy) return;
      busy = true;
      try {
        await refresh();
        if (!detailId) await pollStats();
      } finally {
        busy = false;
      }
    }, 2500);

    // Close the row menu on any click outside it, or on Escape.
    const onDown = (e: PointerEvent) => {
      if (menuFor && !(e.target as HTMLElement).closest?.(".menu-wrap")) menuFor = null;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") menuFor = null;
    };
    const onScroll = () => (menuFor = null);
    window.addEventListener("pointerdown", onDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      clearInterval(statsTimer);
      window.removeEventListener("pointerdown", onDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", onScroll, true);
    };
  });

  const isRunning = (c: Container) => c.state === "running";
  let runningCount = $derived(items.filter(isRunning).length);

  // Case-insensitive match on name, image, short ID or published port, then the state filter.
  let filtered = $derived.by(() => {
    const needle = q.trim().toLowerCase();
    return items.filter((c) => {
      if (filter === "running" && !isRunning(c)) return false;
      if (filter === "stopped" && isRunning(c)) return false;
      if (!needle) return true;
      return (
        c.name.toLowerCase().includes(needle) ||
        c.image.toLowerCase().includes(needle) ||
        c.id.startsWith(needle) ||
        c.ports.some((p) => String(p.public_port ?? "").includes(needle))
      );
    });
  });

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
  function openPort(port: number) {
    api.openUrl(`http://localhost:${port}`).catch((e) => notify("error", String(e)));
  }
  async function act(fn: () => Promise<unknown>, msg: string) {
    menuFor = null;
    if (await guard(fn, msg)) refresh();
  }
  // Like `act`, but asks for confirmation first (restart / stop / remove).
  async function confirmAct(opts: { title?: string; message: string; confirmText?: string; danger?: boolean }, fn: () => Promise<unknown>, msg: string) {
    menuFor = null;
    if (!(await askConfirm(opts))) return;
    await act(fn, msg);
  }
  const stopC = (c: Container) =>
    confirmAct({ title: `¿Detener "${c.name}"?`, message: "El contenedor se detendrá; podrás volver a iniciarlo.", confirmText: "Detener" }, () => api.stopContainer(c.id), `${c.name} detenido`);
  const restartC = (c: Container) =>
    confirmAct({ title: `¿Reiniciar "${c.name}"?`, message: "Se detendrá y volverá a arrancar.", confirmText: "Reiniciar", danger: false }, () => api.restartContainer(c.id), `${c.name} reiniciado`);
  const removeC = (c: Container) =>
    confirmAct({ title: `¿Eliminar "${c.name}"?`, message: "Se borrará el contenedor (sus volúmenes con nombre se conservan). No se puede deshacer.", confirmText: "Eliminar" }, () => api.removeContainer(c.id, true), `${c.name} eliminado`);

  // ── selection + bulk actions ──
  let sel = $state<Record<string, boolean>>({});
  let selectedList = $derived(items.filter((c) => sel[c.id]));
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
  // Run `fn` over several containers and summarise the outcome in one toast.
  async function bulk(list: Container[], fn: (c: Container) => Promise<unknown>, done: string) {
    let ok = 0;
    const failed: string[] = [];
    for (const c of list) {
      try {
        await fn(c);
        ok++;
      } catch {
        failed.push(c.name);
      }
    }
    if (failed.length) notify("error", `${ok} ${done}; fallaron: ${failed.join(", ")}`);
    else notify("success", `${ok} ${done}`);
    refresh();
  }
  async function bulkStart(list = selectedList) {
    await bulk(list.filter((c) => !isRunning(c)), (c) => (c.state === "paused" ? api.unpauseContainer(c.id) : api.startContainer(c.id)), "iniciados");
  }
  async function bulkStop(list = selectedList, title?: string) {
    const targets = list.filter((c) => isRunning(c) || c.state === "paused");
    if (!targets.length) return notify("info", "Ninguno está en ejecución.");
    const ok = await askConfirm({
      title: title ?? `¿Detener ${targets.length} ${targets.length === 1 ? "contenedor" : "contenedores"}?`,
      message: targets.map((c) => c.name).join("\n"),
      confirmText: "Detener",
    });
    if (ok) await bulk(targets, (c) => api.stopContainer(c.id), "detenidos");
  }
  async function bulkRemove() {
    const list = selectedList;
    if (!list.length) return;
    const ok = await askConfirm({
      title: `¿Eliminar ${list.length} ${list.length === 1 ? "contenedor" : "contenedores"}?`,
      message: `${list.map((c) => c.name).join("\n")}\n\nSe detendrán y borrarán. No se puede deshacer.`,
      confirmText: `Eliminar ${list.length}`,
    });
    if (!ok) return;
    sel = {};
    await bulk(list, (c) => api.removeContainer(c.id, true), "eliminados");
  }
</script>

{#if detail}
  <ContainerDetails container={detail} onBack={() => (detailId = null)} />
{:else}
  <div class="page-head">
    <span class="ph-icon"><Icon name="container" /></span>
    <h2>Contenedores</h2>
    <p class="page-sub">
      {items.length} {items.length === 1 ? "contenedor" : "contenedores"} · {runningCount} en ejecución · se actualiza solo
    </p>
    <div class="grow"></div>
    <button class="btn primary" onclick={() => (showRun = true)}><Icon name="play" /> Ejecutar contenedor</button>
  </div>

  <div class="toolbar">
    <label class="search-box">
      <Icon name="search" />
      <input type="search" placeholder="Buscar por nombre, imagen o puerto" aria-label="Buscar contenedores" bind:value={q} />
    </label>
    <div class="segmented" role="group" aria-label="Filtrar por estado">
      <button aria-pressed={filter === "all"} onclick={() => (filter = "all")}>Todos<span class="seg-count">{items.length}</span></button>
      <button aria-pressed={filter === "running"} onclick={() => (filter = "running")}>En ejecución<span class="seg-count">{runningCount}</span></button>
      <button aria-pressed={filter === "stopped"} onclick={() => (filter = "stopped")}>Detenidos<span class="seg-count">{items.length - runningCount}</span></button>
    </div>
    <div class="grow"></div>
    <span class="live">En vivo</span>
  </div>

  {#if selectedList.length}
    <div class="bulk-bar" role="region" aria-label="Acciones en lote">
      <span class="bulk-count">{selectedList.length} {selectedList.length === 1 ? "seleccionado" : "seleccionados"}</span>
      <button class="btn" onclick={() => bulkStart()}><Icon name="play" /> Iniciar</button>
      <button class="btn" onclick={() => bulkStop()}><Icon name="stop" /> Detener</button>
      <button class="btn danger" onclick={bulkRemove}><Icon name="trash" /> Eliminar…</button>
      <button class="btn ghost" onclick={() => (sel = {})}>Cancelar</button>
    </div>
  {/if}

  {#if filtered.length === 0}
    <div class="empty">
      {#if !loaded}
        <span class="spinner"></span>
      {:else if items.length > 0}
        <div class="big">Ningún contenedor coincide</div>
        Prueba con otro texto o cambia el filtro de estado.
        <div class="btn-row">
          <button class="btn" onclick={() => ((q = ""), (filter = "all"))}>Mostrar todos</button>
        </div>
      {:else}
        <span class="empty-ic"><Icon name="container" /></span>
        <div class="big">Aún no tienes contenedores</div>
        Ejecuta uno desde una imagen, o levanta un proyecto con su docker-compose.yml.
        <div class="btn-row">
          <button class="btn primary lg" onclick={() => (showRun = true)}>Ejecutar contenedor</button>
          <button class="btn lg" onclick={() => route.set("compose")}>Abrir Compose</button>
        </div>
      {/if}
    </div>
  {:else}
    <div class="table-wrap">
      <table style="min-width:980px">
        <thead>
          <tr>
            <th style="width:44px;text-align:center">
              <input type="checkbox" checked={allSelected} onchange={toggleAll} aria-label="Seleccionar todos" />
            </th>
            <th>Nombre</th>
            <th style="width:170px">Estado</th>
            <th>Imagen</th>
            <th>Puertos</th>
            <th class="num" style="width:72px">CPU</th>
            <th class="num" style="width:90px">Memoria</th>
            <th class="num" style="width:132px">Acciones</th>
          </tr>
        </thead>
        <tbody>
          {#each grouped.groups as [proj, list] (proj)}
            {@const up = list.filter(isRunning).length}
            <tr class="group-row">
              <td colspan="8">
                <div style="display:flex;align-items:center;gap:8px">
                  <button
                    class="group-head"
                    style="flex:1"
                    aria-expanded={!collapsed[proj]}
                    onclick={() => toggleGroup(proj)}
                  >
                    <span class="group-chev {collapsed[proj] ? '' : 'open'}" aria-hidden="true">▸</span>
                    <span style="color:var(--accent);display:inline-flex"><Icon name="compose" size={16} /></span>
                    <b>{proj}</b>
                    <span class="group-count">Compose · {up} de {list.length} {list.length === 1 ? "servicio activo" : "servicios activos"}</span>
                  </button>
                  {#if up < list.length}
                    <button class="btn" style="min-height:32px" onclick={() => bulkStart(list)}>Iniciar todo</button>
                  {/if}
                  {#if up > 0}
                    <button class="btn" style="min-height:32px" onclick={() => bulkStop(list, `¿Detener el proyecto "${proj}"?`)}>Detener todo</button>
                  {/if}
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
{/if}

{#snippet row(c: Container, inGroup: boolean)}
  {@const st = containerState(c)}
  {@const ports = publishedPorts(c.ports)}
  <tr style={sel[c.id] ? "background:#14231f" : ""}>
    <td style="text-align:center">
      <input type="checkbox" checked={!!sel[c.id]} onchange={() => toggleSel(c.id)} aria-label="Seleccionar {c.name}" />
    </td>
    <td>
      <div class="cell-main" style={inGroup ? "padding-left:22px" : ""}>
        <button class="row-open ellip" style="display:block;max-width:280px" title="Ver logs, terminal y detalles" onclick={() => (detailId = c.id)}>{c.name}</button>
        <button class="hash-copy mono" style="color:var(--faint);font-size:12px" title="Copiar ID completo" onclick={() => copyText(c.id, "ID del contenedor copiado")}>{shortId(c.id)}</button>
      </div>
    </td>
    <td>
      <span class="badge {st.kind}" title={c.status}><span class="b-dot"></span>{st.label}</span>
      {#if stateSince(c)}<span class="cell-sub">{stateSince(c)}</span>{/if}
    </td>
    <td>
      <button class="hash-copy mono ellip" style="max-width:240px;display:block" title="Ver la imagen" onclick={() => openImage(c)}>{c.image}</button>
    </td>
    <td>
      {#if ports.length}
        <div class="chips">
          {#each ports as p (p.label)}
            {#if p.web && isRunning(c)}
              <button class="port-chip" title="Abrir http://localhost:{p.port}" onclick={() => openPort(p.port)}>{p.label}<Icon name="external" size={11} /></button>
            {:else}
              <span class="port-chip" style="cursor:default">{p.label}</span>
            {/if}
          {/each}
        </div>
      {:else}
        <span style="color:var(--faint)">—</span>
      {/if}
    </td>
    <td class="num">{isRunning(c) && liveStats[c.id] ? liveStats[c.id].cpu_percent.toFixed(1) + "%" : "—"}</td>
    <td class="num">{isRunning(c) && liveStats[c.id] ? bytes(liveStats[c.id].mem_usage) : "—"}</td>
    <td>
      <div class="cell-actions">
        {#if isRunning(c)}
          <button class="btn icon ghost" title="Detener" aria-label="Detener {c.name}" onclick={() => stopC(c)}><Icon name="stop" /></button>
          <button class="btn icon ghost" title="Reiniciar" aria-label="Reiniciar {c.name}" onclick={() => restartC(c)}><Icon name="restart" /></button>
        {:else if c.state === "paused"}
          <button class="btn icon ghost" style="color:var(--ok)" title="Reanudar" aria-label="Reanudar {c.name}" onclick={() => act(() => api.unpauseContainer(c.id), `${c.name} reanudado`)}><Icon name="play" /></button>
          <button class="btn icon ghost" title="Detener" aria-label="Detener {c.name}" onclick={() => stopC(c)}><Icon name="stop" /></button>
        {:else}
          <button class="btn icon ghost" style="color:var(--ok)" title="Iniciar" aria-label="Iniciar {c.name}" onclick={() => act(() => api.startContainer(c.id), `${c.name} iniciado`)}><Icon name="play" /></button>
          <button class="btn icon ghost" title="Eliminar" aria-label="Eliminar {c.name}" onclick={() => removeC(c)}><Icon name="trash" /></button>
        {/if}
        <div class="menu-wrap">
          <button
            class="btn icon ghost"
            title="Más acciones"
            aria-label="Más acciones para {c.name}"
            aria-haspopup="menu"
            aria-expanded={menuFor === c.id}
            onclick={(e) => toggleMenu(c.id, e.currentTarget)}
          ><Icon name="more" /></button>
          {#if menuFor === c.id}
            <div
              class="menu"
              role="menu"
              style="position:fixed;right:{menuPos.right}px;{menuPos.up ? `bottom:${window.innerHeight - menuPos.top}px;top:auto` : `top:${menuPos.top}px`}"
            >
              <button role="menuitem" onclick={() => ((menuFor = null), (detailId = c.id))}><Icon name="terminal" size={15} /> Logs y terminal</button>
              {#if isRunning(c)}
                <button role="menuitem" onclick={() => act(() => api.pauseContainer(c.id), `${c.name} en pausa`)}><Icon name="pause" size={15} /> Pausar</button>
              {/if}
              <button role="menuitem" onclick={() => ((menuFor = null), copyText(c.id, "ID del contenedor copiado"))}><Icon name="copy" size={15} /> Copiar ID</button>
              {#if isRunning(c) || c.state === "paused"}
                <hr />
                <button role="menuitem" class="danger" onclick={() => removeC(c)}><Icon name="trash" size={15} /> Eliminar…</button>
              {/if}
            </div>
          {/if}
        </div>
      </div>
    </td>
  </tr>
{/snippet}

{#if showRun}
  <RunContainer onClose={() => (showRun = false)} onCreated={load} />
{/if}
