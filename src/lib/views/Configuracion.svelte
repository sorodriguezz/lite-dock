<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { engine, notify } from "../stores";
  import { api, listen, type UnlistenFn } from "../api";
  import { stripAnsi } from "../format";
  import Modal from "../components/Modal.svelte";
  import LogConsole from "../components/LogConsole.svelte";
  import Icon from "../components/Icon.svelte";

  let cliEnabled = $state(false);
  let cliBusy = $state(false);

  let showUpdate = $state(false);
  let updating = $state(false);
  let updateLines = $state<string[]>([]);
  let updateUnlisten: UnlistenFn | undefined;

  let wslMemMb = $state("");
  let wslReclaim = $state(false);
  let wslBusy = $state(false);

  onMount(() => {
    refreshCli();
    loadWsl();
  });
  onDestroy(() => updateUnlisten?.());

  // WSL needs a floor of RAM to boot the kernel + dockerd + a container.
  const MIN_WSL_MB = 512;

  async function loadWsl() {
    try {
      const c = await api.wslConfigGet();
      wslMemMb = c.memory_mb ? String(c.memory_mb) : "";
      wslReclaim = c.auto_reclaim;
    } catch {
      /* ignore */
    }
  }

  async function applyWsl() {
    const raw = wslMemMb.trim().replace(",", ".");
    let memoryMb: number | null = null;
    if (raw !== "") {
      const n = Math.round(parseFloat(raw));
      if (!isFinite(n) || n <= 0) {
        notify("error", "Memoria inválida (en MB, p. ej. 1024)");
        return;
      }
      if (n < MIN_WSL_MB) {
        notify("error", `El mínimo es ${MIN_WSL_MB} MB para que WSL y Docker funcionen bien.`);
        return;
      }
      memoryMb = n;
    }
    if (
      !confirm(
        "Esto reiniciará WSL: apaga y enciende el motor (afecta a todas tus distros WSL). ¿Continuar?",
      )
    ) {
      return;
    }
    wslBusy = true;
    try {
      await api.wslConfigApply(memoryMb, wslReclaim);
      notify("success", "WSL configurado y motor reiniciado");
      engine.set(await api.engineStatus());
    } catch (e) {
      notify("error", String(e));
    }
    wslBusy = false;
  }

  async function refreshCli() {
    try {
      cliEnabled = await api.cliStatus();
    } catch {
      /* ignore */
    }
  }

  async function toggleCli() {
    cliBusy = true;
    try {
      if (cliEnabled) {
        await api.disableDockerCli();
        cliEnabled = false;
        notify("info", "Comandos docker deshabilitados. Abre una terminal nueva para aplicar el cambio.");
      } else {
        const host = await api.enableDockerCli();
        cliEnabled = true;
        notify("success", `Comandos docker habilitados (DOCKER_HOST=${host}). Abre una terminal nueva.`);
      }
    } catch (e) {
      notify("error", String(e));
    } finally {
      cliBusy = false;
    }
  }

  async function startUpdate() {
    showUpdate = true;
    updating = true;
    updateLines = [];
    updateUnlisten?.();
    updateUnlisten = await listen<{ line: string; stream: string }>("engine-update", (e) => {
      if (!e.payload.line.startsWith("__EXIT__")) {
        updateLines = [...updateLines, stripAnsi(e.payload.line)].slice(-3000);
      }
    });
    try {
      const code = await api.engineUpdate();
      if (code === 0) notify("success", "Motor actualizado");
      else notify("error", `La actualización terminó con código ${code}`);
      engine.set(await api.engineStatus());
    } catch (e) {
      notify("error", String(e));
    }
    updating = false;
  }
</script>

<div class="page-head">
  <span class="ph-icon"><Icon name="settings" /></span>
  <h2>Configuración</h2>
</div>

<div class="grid" style="grid-template-columns:1fr 1fr;gap:14px">
  <div class="card">
    <div class="label" style="margin-bottom:12px">Comandos docker en tu terminal</div>
    <button
      class="btn {cliEnabled ? 'ok' : ''}"
      onclick={toggleCli}
      disabled={cliBusy}
      title={cliEnabled
        ? "Clic para desactivar docker / docker compose en tus terminales"
        : "Habilita docker / docker compose usando LiteDock en tus terminales"}
    >
      {#if cliBusy}
        <span class="spinner"></span> {cliEnabled ? "Desactivando…" : "Activando…"}
      {:else if cliEnabled}
        ✓ Comandos docker activos
      {:else}
        Habilitar comandos docker
      {/if}
    </button>
    <div style="color:var(--faint);font-size:12px;margin-top:8px">
      Conecta <code>docker</code> / <code>docker compose</code> a LiteDock en tus terminales.
      Cuando está activo, púlsalo de nuevo para desactivarlo. El cambio aplica en terminales nuevas.
    </div>
  </div>

  <div class="card">
    <div class="label" style="margin-bottom:12px">Motor</div>
    <div style="color:var(--muted);font-size:13px;margin-bottom:10px">
      Versión actual: <b style="color:var(--text)">{$engine.version ?? "—"}</b>
    </div>
    <button class="btn primary" onclick={startUpdate} disabled={updating}>
      {#if updating}<span class="spinner"></span>{/if} Actualizar Docker
    </button>
    <div style="color:var(--faint);font-size:12px;margin-top:8px">
      Instala la última versión disponible del motor (Docker, BuildKit y Compose) y lo reinicia.
    </div>
  </div>
</div>

<div class="card" style="margin-top:14px">
  <div class="label" style="margin-bottom:12px">Memoria de WSL (motor)</div>
  <div style="display:flex;gap:14px;align-items:flex-end;flex-wrap:wrap">
    <div class="field" style="margin:0">
      <label for="wslmem">Límite de RAM (MB)</label>
      <input id="wslmem" type="text" inputmode="numeric" placeholder="p. ej. 1024 (mín. 512)" bind:value={wslMemMb} style="width:170px" />
    </div>
    <label style="display:flex;align-items:center;gap:7px;color:var(--muted);font-size:13px;padding-bottom:9px">
      <input type="checkbox" bind:checked={wslReclaim} style="width:auto;margin:0" />
      Devolver memoria libre a Windows
    </label>
    <button class="btn primary" onclick={applyWsl} disabled={wslBusy}>
      {#if wslBusy}<span class="spinner"></span>{/if} Guardar y aplicar
    </button>
  </div>
  <div style="color:var(--faint);font-size:12px;margin-top:8px">
    Limita cuánta RAM puede usar el motor (es lo que reduce el consumo de <code>vmmemWSL</code>).
    Mínimo 512 MB; recomendado ≥ 1024 MB para cargas reales. Deja el campo vacío para quitar el límite.
    Al aplicar se reinicia WSL: apaga y enciende el motor (afecta a todas tus distros WSL).
  </div>
</div>

{#if showUpdate}
  <Modal title="Actualizar el motor" onClose={() => { if (!updating) showUpdate = false; }}>
    <p style="color:var(--muted);margin-top:0">
      Actualizando Docker, BuildKit y Compose a la última versión disponible y reiniciando el motor…
    </p>
    <div style="height:40vh"><LogConsole lines={updateLines} placeholder="Iniciando…" /></div>
    {#snippet footer()}
      <button class="btn" onclick={() => (showUpdate = false)} disabled={updating}>
        {updating ? "Trabajando…" : "Cerrar"}
      </button>
    {/snippet}
  </Modal>
{/if}
