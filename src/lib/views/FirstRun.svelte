<script lang="ts">
  import { onMount } from "svelte";
  import { api, listen, type UnlistenFn } from "../api";
  import { engine } from "../stores";
  import LogConsole from "../components/LogConsole.svelte";
  import type { SetupProgress, WslStatus } from "../types";

  interface Props {
    onReady: () => void;
  }
  let { onReady }: Props = $props();

  let status = $state<WslStatus | null>(null);
  let running = $state(false);
  let steps = $state<SetupProgress[]>([]);
  let rebootNeeded = $state(false);
  let failed = $state(false);
  let logLines = $state<string[]>([]);
  let logsOpen = $state(false);
  let unlisten: UnlistenFn | undefined;
  let unlistenEngine: UnlistenFn | undefined;

  const LABELS: Record<string, string> = {
    detect: "Comprobar WSL2",
    wsl: "Preparar WSL2",
    import: "Importar el motor",
    start: "Arrancar el motor",
    verify: "Verificar",
    done: "Listo",
  };

  function appendLog(line: string) {
    logLines = [...logLines, line].slice(-1000);
  }

  onMount(() => {
    detect();
    listen<SetupProgress>("setup-progress", (e) => {
      const p = e.payload;
      const idx = steps.findIndex((s) => s.step === p.step);
      if (idx >= 0) {
        steps[idx] = p;
        steps = [...steps];
      } else {
        steps = [...steps, p];
      }
    }).then((u) => (unlisten = u));
    listen<string>("engine-log", (e) => appendLog(e.payload)).then(
      (u) => (unlistenEngine = u),
    );
    return () => {
      unlisten?.();
      unlistenEngine?.();
    };
  });

  async function detect() {
    try {
      status = await api.setupDetect();
    } catch (e) {
      appendLog("detect: " + String(e));
    }
  }

  function failSetup(msg: string) {
    failed = true;
    appendLog("");
    appendLog("⚠ " + msg);
    logsOpen = true; // surface the logs automatically on failure
  }

  async function run() {
    running = true;
    failed = false;
    steps = [];
    rebootNeeded = false;
    logLines = [];
    try {
      const res = await api.setupRun();
      if (res.needs_reboot) {
        rebootNeeded = true;
      } else if (res.ok) {
        engine.set(await api.engineStatus());
        onReady();
        return;
      } else {
        failSetup(res.message);
      }
    } catch (e) {
      failSetup(String(e));
    }
    running = false;
  }

  async function refreshLogs() {
    try {
      const t = await api.engineLogs();
      if (t) logLines = t.split(/\r?\n/);
      logsOpen = true;
    } catch (e) {
      appendLog(String(e));
    }
  }

  let isInstalled = $derived(!!status && status.wsl2_ready && status.distro_imported);
  let primaryLabel = $derived(isInstalled ? "Iniciar motor" : "Instalar y preparar");
  // Block install while hardware virtualization is off — WSL2 cannot run without it.
  let virtOk = $derived(!status || status.virtualization_enabled);
</script>

<div class="firstrun">
  <div class="panel">
    <svg width="44" height="44" viewBox="0 0 32 32" fill="none" aria-hidden="true">
      <rect x="1" y="1" width="30" height="30" rx="8" fill="url(#fg)" />
      <rect x="9" y="14" width="4" height="4" rx="1" fill="#04130f" />
      <rect x="14" y="14" width="4" height="4" rx="1" fill="#04130f" />
      <rect x="19" y="14" width="4" height="4" rx="1" fill="#04130f" />
      <rect x="11.5" y="9" width="4" height="4" rx="1" fill="#04130f" />
      <rect x="16.5" y="9" width="4" height="4" rx="1" fill="#04130f" />
      <path d="M7 19h18a6 6 0 0 1-6 5h-6a6 6 0 0 1-6-5z" fill="#04130f" />
      <defs>
        <linearGradient id="fg" x1="0" y1="0" x2="32" y2="32">
          <stop stop-color="#2dd4bf" />
          <stop offset="1" stop-color="#38bdf8" />
        </linearGradient>
      </defs>
    </svg>

    <h1>Bienvenido a LiteDock</h1>
    <p class="sub">Tu gestor de contenedores Docker, liviano y todo-en-uno.</p>

    {#if status}
      <div class="step {isInstalled ? 'ok' : ''}">
        <div class="ic">{isInstalled ? "✓" : "ⓘ"}</div>
        <div class="txt">
          <b>{status.message}</b>
          <small>
            WSL2: {status.wsl2_ready ? "listo" : "pendiente"} · Motor: {status.distro_imported
              ? "importado"
              : "sin importar"}
          </small>
        </div>
      </div>
    {/if}

    {#if status && !status.virtualization_enabled}
      <div class="step error" style="align-items:flex-start">
        <div class="ic">⚠</div>
        <div class="txt">
          <b>La virtualización por hardware está desactivada</b>
          <small>WSL2 la necesita para funcionar. Actívala así:</small>
          <ol class="virt-steps">
            <li>Reinicia el PC y entra a la BIOS/UEFI (suele ser Supr, F2, F10 o Esc al encender).</li>
            <li>Activa <b>Intel VT-x</b> o <b>AMD-V (SVM)</b> — a veces aparece como “Virtualization Technology”.</li>
            <li>Guarda los cambios y vuelve a Windows.</li>
            <li>Activa la característica <b>“Plataforma de máquina virtual”</b> en “Activar o desactivar características de Windows”.</li>
          </ol>
          <small>Después pulsa “Volver a comprobar”.</small>
        </div>
      </div>
    {/if}

    {#if steps.length}
      <div class="steps">
        {#each steps as s (s.step)}
          <div class="step {s.status === 'ok' ? 'ok' : s.status === 'error' ? 'error' : 'running'}">
            <div class="ic">
              {#if s.status === "running"}<span class="spinner"></span>{:else if s.status === "ok"}✓{:else if s.status === "error"}✕{:else}ⓘ{/if}
            </div>
            <div class="txt">
              <b>{LABELS[s.step] ?? s.step}</b>
              <small>{s.message}</small>
            </div>
          </div>
        {/each}
      </div>
    {/if}

    {#if rebootNeeded}
      <p style="color:var(--warn)">
        WSL2 se instaló. Reinicia Windows y vuelve a abrir LiteDock para terminar.
      </p>
    {/if}

    {#if failed}
      <p class="setup-failed">No se pudo completar el arranque — revisa los logs del motor abajo.</p>
    {/if}

    {#if logLines.length || failed}
      <div class="log-acc">
        <button class="log-acc-head" onclick={() => (logsOpen = !logsOpen)}>
          <span class="chev">{logsOpen ? "▾" : "▸"}</span>
          Logs del motor
          {#if failed}<span class="tag-error">error</span>{/if}
        </button>
        {#if logsOpen}
          <div style="height:30vh; margin-top:8px">
            <LogConsole lines={logLines} placeholder="Sin logs todavía…" />
          </div>
          <div style="margin-top:6px">
            <button class="btn" onclick={refreshLogs}>Refrescar logs</button>
          </div>
        {/if}
      </div>
    {/if}

    <div class="btn-row" style="margin-top:12px">
      <button class="btn primary" onclick={run} disabled={running || !virtOk}>
        {#if running}<span class="spinner"></span>{/if}
        {primaryLabel}
      </button>
      <button class="btn" onclick={detect} disabled={running}>Volver a comprobar</button>
    </div>
  </div>
</div>
