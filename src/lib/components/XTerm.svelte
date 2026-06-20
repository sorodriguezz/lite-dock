<script lang="ts">
  // A real terminal emulator (xterm.js) so interactive programs work: top/htop,
  // vim/nano, Ctrl+C, arrow-key history — every keystroke is forwarded raw to the
  // shell and the shell's full-screen redraws render correctly. Fixed at 80×24 to
  // match the exec PTY's default size, so no backend resize is needed.
  import { onMount, onDestroy } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import "@xterm/xterm/css/xterm.css";

  interface Props {
    /** Called with the raw bytes for any key the user presses. */
    ondata?: (d: string) => void;
    cols?: number;
    rows?: number;
  }
  let { ondata, cols = 80, rows = 24 }: Props = $props();

  let host = $state<HTMLDivElement>();
  let term: Terminal | undefined;

  // Imperative API exposed to the parent via bind:this.
  export function write(d: string) {
    term?.write(d);
  }
  export function clear() {
    term?.clear();
  }
  export function reset() {
    term?.reset();
  }
  export function focus() {
    term?.focus();
  }

  onMount(() => {
    term = new Terminal({
      cols,
      rows,
      fontFamily: '"Cascadia Code", "JetBrains Mono", "SF Mono", Consolas, monospace',
      fontSize: 13,
      cursorBlink: true,
      scrollback: 5000,
      theme: {
        background: "#060810",
        foreground: "#cdd6e6",
        cursor: "#2dd4bf",
        cursorAccent: "#060810",
        selectionBackground: "#2a3450",
      },
    });
    if (host) term.open(host);
    term.onData((d) => ondata?.(d));
    term.focus();
  });
  onDestroy(() => term?.dispose());
</script>

<div class="xterm-host" bind:this={host}></div>

<style>
  .xterm-host {
    height: 100%;
    background: #060810;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px 6px 8px 10px;
    overflow: hidden;
    box-sizing: border-box;
  }
  /* xterm injects its own elements; keep the viewport scrollbar subtle. */
  .xterm-host :global(.xterm-viewport)::-webkit-scrollbar {
    width: 9px;
  }
  .xterm-host :global(.xterm-viewport)::-webkit-scrollbar-thumb {
    background: #2a3450;
    border-radius: 6px;
  }
</style>
