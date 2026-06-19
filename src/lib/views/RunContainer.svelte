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

  let images = $state<Image[]>([]);
  let image = $state("");
  let name = $state("");
  let ports = $state("");
  let env = $state("");
  let volumes = $state("");
  let restart = $state("no");
  let creating = $state(false);

  // Tagged images the user can actually launch.
  let runnable = $derived(
    images
      .map((i) => primaryTag(i.tags))
      .filter((t) => t && t !== "<none>:<none>"),
  );

  onMount(async () => {
    try {
      images = await api.listImages();
      if (!image && runnable.length) image = runnable[0];
    } catch (e) {
      notify("error", String(e));
    }
  });

  function lines(s: string): string[] {
    return s
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l.length > 0);
  }

  async function run() {
    if (!image.trim()) {
      notify("error", "Elige una imagen");
      return;
    }
    creating = true;
    try {
      await api.runContainer({
        image: image.trim(),
        name: name.trim() || undefined,
        ports: lines(ports),
        env: lines(env),
        volumes: lines(volumes),
        restart,
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
    <label for="rc-img">Imagen</label>
    {#if runnable.length}
      <select id="rc-img" bind:value={image}>
        {#each runnable as t (t)}
          <option value={t}>{t}</option>
        {/each}
      </select>
    {:else}
      <input id="rc-img" type="text" placeholder="p. ej. postgres:16-alpine" bind:value={image} />
      <div style="color:var(--faint);font-size:12px;margin-top:6px">
        No hay imágenes descargadas. Escribe una referencia o descárgala primero en “Imágenes”.
      </div>
    {/if}
  </div>

  <div class="field">
    <label for="rc-name">Nombre (opcional)</label>
    <input id="rc-name" type="text" placeholder="p. ej. mi-postgres" bind:value={name} />
  </div>

  <div class="grid" style="grid-template-columns:1fr 1fr;gap:14px">
    <div class="field">
      <label for="rc-ports">Puertos · host:contenedor (uno por línea)</label>
      <textarea id="rc-ports" rows="3" placeholder={"5432:5432\n8080:80/tcp"} bind:value={ports}></textarea>
    </div>
    <div class="field">
      <label for="rc-vol">Volúmenes · origen:destino (uno por línea)</label>
      <textarea id="rc-vol" rows="3" placeholder={"mis-datos:/var/lib/postgresql/data"} bind:value={volumes}></textarea>
    </div>
  </div>

  <div class="field">
    <label for="rc-env">Variables de entorno · CLAVE=valor (una por línea)</label>
    <textarea id="rc-env" rows="3" placeholder={"POSTGRES_PASSWORD=secret\nPOSTGRES_USER=admin"} bind:value={env}></textarea>
  </div>

  <div class="field" style="margin-bottom:0">
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
      {#if creating}<span class="spinner"></span>{/if} Ejecutar
    </button>
  {/snippet}
</Modal>
