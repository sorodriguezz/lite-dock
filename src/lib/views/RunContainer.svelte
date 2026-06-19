<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { notify } from "../stores";
  import type { Image } from "../types";
  import { primaryTag } from "../format";
  import Modal from "../components/Modal.svelte";

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
  <div class="field" style="margin-bottom:8px">
    <input
      id="rc-img"
      list="rc-img-list"
      type="text"
      placeholder="p. ej. nginx:latest"
      bind:value={image}
    />
    <datalist id="rc-img-list">
      {#each runnable as t (t)}<option value={t}></option>{/each}
    </datalist>
  </div>
  <label class="rc-toggle">
    <input type="checkbox" bind:checked={pull} />
    Siempre traer la imagen (pull)
  </label>

  <div class="rc-title">Puertos publicados</div>
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
  <button class="rc-add" onclick={() => portRows.push({ id: uid(), host: "", container: "", proto: "tcp" })}>
    + Publicar un puerto
  </button>

  <div class="rc-title">Volúmenes</div>
  {#each volRows as row (row.id)}
    <div class="rc-row">
      <input class="rc-grow" type="text" placeholder="volumen o ruta del host" bind:value={row.source} />
      <span class="rc-sep">→</span>
      <input class="rc-grow" type="text" placeholder="/ruta/en/contenedor" bind:value={row.target} />
      <button class="rc-del" title="Quitar" aria-label="Quitar volumen" onclick={() => (volRows = volRows.filter((p) => p.id !== row.id))}>✕</button>
    </div>
  {/each}
  <button class="rc-add" onclick={() => volRows.push({ id: uid(), source: "", target: "" })}>
    + Añadir volumen
  </button>

  <div class="rc-title">Variables de entorno</div>
  {#each envRows as row (row.id)}
    <div class="rc-row">
      <input class="rc-grow" type="text" placeholder="CLAVE" bind:value={row.key} />
      <span class="rc-sep">=</span>
      <input class="rc-grow" type="text" placeholder="valor" bind:value={row.value} />
      <button class="rc-del" title="Quitar" aria-label="Quitar variable" onclick={() => (envRows = envRows.filter((p) => p.id !== row.id))}>✕</button>
    </div>
  {/each}
  <button class="rc-add" onclick={() => envRows.push({ id: uid(), key: "", value: "" })}>
    + Añadir variable
  </button>

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
  .rc-port {
    width: 110px;
    flex: none;
  }
  .rc-grow {
    flex: 1;
    min-width: 0;
  }
  .rc-proto {
    width: 80px;
    flex: none;
  }
  .rc-sep {
    color: var(--faint);
    flex: none;
  }
  .rc-del {
    flex: none;
    width: 30px;
    height: 30px;
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
</style>
