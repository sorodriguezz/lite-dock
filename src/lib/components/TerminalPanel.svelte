<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api, listen, type UnlistenFn } from "../api";
  import { notify } from "../stores";
  import LogConsole from "./LogConsole.svelte";

  type Kind = "host" | "engine";

  let open = $state(false);
  let kind = $state<Kind>("host");
  let session = $state<string | null>(null);
  let starting = $state(false);
  let lines = $state<string[]>([]);
  let input = $state("");
  let hist = $state<string[]>([]);
  let histPos = $state(-1);

  let outUnlisten: UnlistenFn | undefined;
  let exitUnlisten: UnlistenFn | undefined;

  onMount(async () => {
    // Listeners filter by the live `session`, so they survive shell restarts.
    outUnlisten = await listen<{ session: string; line: string }>("terminal-output", (e) => {
      if (e.payload.session === session) appendLines(e.payload.line);
    });
    exitUnlisten = await listen<string>("terminal-exit", (e) => {
      if (e.payload === session) {
        lines = [...lines, "\n[sesión finalizada]"];
        session = null;
      }
    });
  });

  onDestroy(() => {
    outUnlisten?.();
    exitUnlisten?.();
    if (session) api.terminalKill(session).catch(() => {});
  });

  async function start() {
    if (session) {
      try {
        await api.terminalKill(session);
      } catch {
        /* ignore */
      }
      session = null;
    }
    lines = [];
    starting = true;
    try {
      session = await api.terminalStart(kind);
    } catch (e) {
      notify("error", String(e));
    }
    starting = false;
  }

  function toggle() {
    open = !open;
    if (open && !session && !starting) start();
  }

  function switchKind(k: Kind) {
    if (k === kind) return;
    kind = k;
    start();
  }

  // Detect the shell's screen-clear escape and wipe the buffer there (real
  // terminal behaviour) instead of matching the typed word.
  const CLEAR_RE = /\x1bc|\x1b\[3J|\x1b\[2J|\x1b\[H\x1b\[[02]?J/g;
  function appendLines(raw: string) {
    CLEAR_RE.lastIndex = 0;
    let cut = -1;
    let m: RegExpExecArray | null;
    while ((m = CLEAR_RE.exec(raw)) !== null) cut = m.index + m[0].length;
    if (cut >= 0) {
      const rest = raw.slice(cut);
      lines = rest ? [rest] : [];
    } else {
      lines = [...lines, raw].slice(-3000);
    }
  }

  async function send(e: Event) {
    e.preventDefault();
    if (!session) return;
    const trimmed = input.trim();
    if (trimmed && hist[hist.length - 1] !== trimmed) hist = [...hist, trimmed].slice(-200);
    histPos = -1;
    const data = input + "\n";
    input = "";
    try {
      await api.terminalWrite(session, data);
    } catch (err) {
      notify("error", String(err));
    }
  }

  // ↑/↓ command history; ↓ past the newest clears the line.
  function onKey(e: KeyboardEvent) {
    if (e.key === "ArrowUp") {
      e.preventDefault();
      if (!hist.length) return;
      histPos = histPos === -1 ? hist.length - 1 : Math.max(0, histPos - 1);
      input = hist[histPos];
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (histPos === -1) return;
      if (histPos >= hist.length - 1) {
        histPos = -1;
        input = "";
      } else {
        histPos++;
        input = hist[histPos];
      }
    }
  }
</script>

<div class="term-dock {open ? 'open' : ''}">
  <div class="term-head">
    <button class="term-toggle" onclick={toggle} title={open ? "Colapsar terminal" : "Abrir terminal"}>
      <svg class="chev {open ? 'up' : ''}" viewBox="0 0 24 24" fill="none" stroke="currentColor"
        stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6" /></svg>
      <svg class="term-glyph" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
        stroke-linecap="round" stroke-linejoin="round"><path d="m4 17 6-6-6-6M12 19h8" /></svg>
      Terminal
    </button>

    {#if open}
      <div class="term-tabs">
        <button class="term-tab {kind === 'host' ? 'active' : ''}" onclick={() => switchKind("host")}>
          PowerShell
        </button>
        <button class="term-tab {kind === 'engine' ? 'active' : ''}" onclick={() => switchKind("engine")}>
          Motor (WSL)
        </button>
      </div>
    {/if}

    <span class="grow"></span>

    {#if open}
      {#if starting}<span class="spinner"></span>{/if}
      <button class="term-icon" title="Limpiar" onclick={() => (lines = [])} disabled={!lines.length}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
          stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18M8 6V4h8v2M6 6l1 14h10l1-14" /></svg>
      </button>
      <button class="term-icon" title="Reiniciar shell" onclick={start}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
          stroke-linecap="round" stroke-linejoin="round"><path d="M23 4v6h-6M1 20v-6h6" /><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" /></svg>
      </button>
    {/if}
  </div>

  {#if open}
    <div class="term-body">
      <LogConsole {lines} placeholder={session ? "Listo. Escribe un comando abajo…" : "Iniciando shell…"} />
    </div>
    <form class="term-input" onsubmit={send}>
      <span class="term-prompt mono">{kind === "host" ? "PS>" : "$"}</span>
      <input class="mono" placeholder="Comando + Enter…" bind:value={input} disabled={!session} onkeydown={onKey} />
      <button class="btn" type="submit" disabled={!session}>Enviar</button>
    </form>
  {/if}
</div>
