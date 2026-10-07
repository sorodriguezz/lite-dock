<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { engine, notify, askConfirm } from "../stores";
  import { api, listen, type UnlistenFn } from "../api";
  import Modal from "../components/Modal.svelte";
  import LogConsole from "../components/LogConsole.svelte";
  import Icon from "../components/Icon.svelte";

  let cliEnabled = $state(false);
  let cliBusy = $state(false);

  let showUpdate = $state(false);
  let updating = $state(false);
  let updateDone = $state(false);
  let updateOk = $state(false);
  let updateLines = $state<string[]>([]);
  let updateUnlisten: UnlistenFn | undefined;

  let wslMemMb = $state("");
  let wslReclaim = $state(false);
  let wslBusy = $state(false);

  let wslDistros = $state<string[]>([]);
  let wslDistro = $state("");
  let wslIntegOn = $state(false);
  let wslIntegBusy = $state(false);

  onMount(() => {
    refreshCli();
    loadWsl();
    loadDistros();
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

  async function loadDistros() {
    try {
      wslDistros = await api.wslListDistros();
      if (!wslDistro && wslDistros.length) {
        wslDistro = wslDistros[0];
        checkIntegration();
      }
    } catch {
      /* ignore */
    }
  }
  async function checkIntegration() {
    if (!wslDistro) return;
    try {
      wslIntegOn = await api.wslIntegrationGet(wslDistro);
    } catch {
      wslIntegOn = false;
    }
  }
  async function toggleIntegration() {
    if (!wslDistro) return;
    wslIntegBusy = true;
    try {
      await api.wslIntegrationSet(wslDistro, !wslIntegOn);
      wslIntegOn = !wslIntegOn;
      notify(
        "success",
        wslIntegOn ? `docker integrado en ${wslDistro}` : `Integración quitada de ${wslDistro}`,
      );
    } catch (e) {
      notify("error", String(e));
    }
    wslIntegBusy = false;
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
      !(await askConfirm({
        title: "Reiniciar WSL",
        message:
          "Esto reiniciará WSL: apaga y enciende el motor (afecta a todas tus distros WSL). ¿Continuar?",
      }))
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
    if (
      !(await askConfirm({
        title: "Actualizar el motor",
        message:
          "Se actualizarán Docker, BuildKit y Compose y luego se reiniciará el motor (los contenedores en ejecución se reiniciarán). ¿Continuar?",
        confirmText: "Actualizar",
      }))
    )
      return;
    showUpdate = true;
    updating = true;
    updateDone = false;
    updateOk = false;
    updateLines = [];
    updateUnlisten?.();
    updateUnlisten = await listen<{ line: string; stream: string }>("engine-update", (e) => {
      if (!e.payload.line.startsWith("__EXIT__")) {
        updateLines = [...updateLines, e.payload.line].slice(-3000);
      }
    });
    try {
      const code = await api.engineUpdate();
      updateOk = code === 0;
      if (code === 0) notify("success", "Motor actualizado");
      else notify("error", `La actualización terminó con código ${code}`);
      engine.set(await api.engineStatus());
    } catch (e) {
      updateOk = false;
      notify("error", String(e));
    }
    updating = false;
    updateDone = true;
  }
</script>

<div class="page-head">
  <span class="ph-icon"><Icon name="settings" /></span>
  <h2>Configuración</h2>
  <p class="page-sub">Motor, WSL y comandos docker</p>
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

<div class="card" style="margin-top:14px">
  <div class="label" style="margin-bottom:12px">Integración con WSL (experimental)</div>
  {#if wslDistros.length}
    <div style="display:flex;gap:12px;align-items:flex-end;flex-wrap:wrap">
      <div class="field" style="margin:0">
        <label for="wsldistro">Distro</label>
        <select id="wsldistro" bind:value={wslDistro} onchange={checkIntegration} style="min-width:200px">
          {#each wslDistros as d (d)}<option value={d}>{d}</option>{/each}
        </select>
      </div>
      <button class="btn {wslIntegOn ? 'ok' : 'primary'}" onclick={toggleIntegration} disabled={wslIntegBusy}>
        {#if wslIntegBusy}<span class="spinner"></span>{/if}
        {wslIntegOn ? "✓ Integrada — quitar" : "Activar docker en esta distro"}
      </button>
    </div>
  {:else}
    <div style="color:var(--faint);font-size:13px">No se detectaron otras distros WSL.</div>
  {/if}
  <div style="color:var(--faint);font-size:12px;margin-top:8px">
    Hace que <code>docker</code> / <code>docker compose</code> funcionen <b>dentro</b> de esa distro WSL,
    reenviando al motor de LiteDock — útil para scripts que ejecutan <code>docker</code> desde WSL.
    Instala unos accesos directos de comandos (pequeños programas que redirigen <code>docker</code> a LiteDock)
    en <code>/usr/local/bin</code> de la distro; "quitar" los elimina.
  </div>
</div>

{#if showUpdate}
  <Modal title="Actualizar el motor" onClose={() => { if (!updating) showUpdate = false; }}>
    <div style="display:flex;align-items:center;gap:10px;margin:0 0 12px;font-size:14px;color:{updating ? 'var(--muted)' : updateOk ? 'var(--ok)' : '#e5484d'}">
      {#if updating}
        <span class="spinner"></span>
        <span>Actualizando Docker, BuildKit y Compose, y reiniciando el motor…</span>
      {:else if updateDone && updateOk}
        <b style="font-size:16px">✓</b>
        <span>Motor actualizado{$engine.version ? ` · v${$engine.version}` : ""}.</span>
      {:else if updateDone}
        <b style="font-size:16px">✕</b>
        <span>La actualización no se completó. Revisa el detalle de abajo.</span>
      {/if}
    </div>
    <div style="height:40vh"><LogConsole lines={updateLines} placeholder="Iniciando…" /></div>
    {#snippet footer()}
      <button class="btn {updateDone && updateOk ? 'primary' : ''}" onclick={() => (showUpdate = false)} disabled={updating}>
        {updating ? "Trabajando…" : "Cerrar"}
      </button>
    {/snippet}
  </Modal>
{/if}
