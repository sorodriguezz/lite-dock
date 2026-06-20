<script module lang="ts">
  // Shared registry so a Ctrl+F hits the top-most visible console, and the
  // global key handler is installed only once for the whole app.
  type Inst = { visible: () => boolean; open: () => void };
  const insts: Inst[] = [];
  let bound = false;
  function ensureGlobalKey() {
    if (bound || typeof window === "undefined") return;
    bound = true;
    window.addEventListener("keydown", (e) => {
      if ((e.ctrlKey || e.metaKey) && (e.key === "f" || e.key === "F")) {
        for (let i = insts.length - 1; i >= 0; i--) {
          if (insts[i].visible()) {
            e.preventDefault();
            insts[i].open();
            return;
          }
        }
      }
    });
  }
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";
  import { parseAnsiSegments, stripAnsi } from "../format";

  interface Props {
    lines: string[];
    placeholder?: string;
  }
  let { lines, placeholder = "" }: Props = $props();

  let el = $state<HTMLDivElement>();
  let wrap = $state<HTMLDivElement>();
  let searchInput = $state<HTMLInputElement>();

  let findOpen = $state(false);
  let query = $state("");
  let current = $state(0);
  let caseSensitive = $state(false);
  let filterMode = $state(false);

  const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

  let displayLines = $derived.by(() => {
    if (!(filterMode && query.trim())) return lines;
    const needle = caseSensitive ? query : query.toLowerCase();
    return lines.filter((l) => {
      const p = stripAnsi(l);
      return (caseSensitive ? p : p.toLowerCase()).includes(needle);
    });
  });

  // Build colour-styled HTML and (when searching) wrap matches in <mark>.
  let rendered = $derived.by(() => {
    const q = query.trim();
    const cs = caseSensitive;
    const cur = current;
    let mi = 0;
    const out: string[] = [];
    for (const raw of displayLines) {
      const segs = parseAnsiSegments(raw);
      if (!q) {
        out.push(segs.map((s) => (s.style ? `<span style="${s.style}">${esc(s.text)}</span>` : esc(s.text))).join(""));
        continue;
      }
      const plain = segs.map((s) => s.text).join("");
      const hay = cs ? plain : plain.toLowerCase();
      const needle = cs ? q : q.toLowerCase();
      const ranges: Array<[number, number, number]> = [];
      let from = 0;
      let idx = hay.indexOf(needle, from);
      while (idx !== -1) {
        ranges.push([idx, idx + needle.length, mi++]);
        from = idx + needle.length;
        idx = hay.indexOf(needle, from);
      }
      let html = "";
      let pos = 0;
      for (const seg of segs) {
        const segEnd = pos + seg.text.length;
        let k = pos;
        while (k < segEnd) {
          const r = ranges.find(([s, e]) => k >= s && k < e);
          if (r) {
            const end = Math.min(segEnd, r[1]);
            const cls = r[2] === cur ? "fnd cur" : "fnd";
            html += `<mark class="${cls}" data-mi="${r[2]}"${seg.style ? ` style="${seg.style}"` : ""}>${esc(plain.slice(k, end))}</mark>`;
            k = end;
          } else {
            let end = segEnd;
            for (const [s] of ranges) if (s > k && s < end) end = s;
            html += seg.style ? `<span style="${seg.style}">${esc(plain.slice(k, end))}</span>` : esc(plain.slice(k, end));
            k = end;
          }
        }
        pos = segEnd;
      }
      out.push(html);
    }
    return { html: out.join("\n"), count: mi };
  });

  // Reset selection when the query/options change.
  $effect(() => {
    void query;
    void caseSensitive;
    void filterMode;
    current = 0;
  });
  // Keep `current` inside the available range.
  $effect(() => {
    if (current >= rendered.count) current = rendered.count > 0 ? rendered.count - 1 : 0;
  });
  // Auto-scroll to the bottom on new lines, unless the user is searching.
  $effect(() => {
    void displayLines.length;
    void rendered.html;
    if (el && !(findOpen && query.trim())) el.scrollTop = el.scrollHeight;
  });
  // Bring the current match into view.
  $effect(() => {
    void current;
    void rendered.html;
    if (!findOpen || !query.trim()) return;
    tick().then(() => {
      el?.querySelector<HTMLElement>(`mark[data-mi="${current}"]`)?.scrollIntoView({ block: "center", inline: "nearest" });
    });
  });

  async function openFind() {
    findOpen = true;
    await tick();
    searchInput?.focus();
    searchInput?.select();
  }
  function closeFind() {
    findOpen = false;
    query = "";
    el?.focus();
  }
  function next() {
    if (rendered.count) current = (current + 1) % rendered.count;
  }
  function prev() {
    if (rendered.count) current = (current - 1 + rendered.count) % rendered.count;
  }
  function onSearchKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) prev();
      else next();
    } else if (e.key === "Escape") {
      e.preventDefault();
      closeFind();
    }
  }

  onMount(() => {
    ensureGlobalKey();
    const inst: Inst = { visible: () => !!wrap && wrap.offsetParent !== null, open: openFind };
    insts.push(inst);
    return () => {
      const i = insts.indexOf(inst);
      if (i >= 0) insts.splice(i, 1);
    };
  });
</script>

<div class="console-wrap" bind:this={wrap}>
  {#if findOpen}
    <div class="find-bar">
      <svg class="fb-ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="11" cy="11" r="7" /><path d="m21 21-4.3-4.3" />
      </svg>
      <input
        class="fb-input mono"
        placeholder="Buscar en el log…"
        bind:value={query}
        bind:this={searchInput}
        onkeydown={onSearchKey}
        spellcheck="false"
        autocomplete="off"
        aria-label="Buscar en el log"
      />
      <span class="fb-count">{rendered.count ? `${current + 1}/${rendered.count}` : query.trim() ? "0" : ""}</span>
      <button class="fb-btn" title="Anterior (Shift+Enter)" aria-label="Anterior" onclick={prev} disabled={!rendered.count}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="m18 15-6-6-6 6" /></svg>
      </button>
      <button class="fb-btn" title="Siguiente (Enter)" aria-label="Siguiente" onclick={next} disabled={!rendered.count}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6" /></svg>
      </button>
      <button class="fb-btn {caseSensitive ? 'on' : ''}" title="Distinguir mayúsculas/minúsculas" aria-label="Distinguir mayúsculas" onclick={() => (caseSensitive = !caseSensitive)}>Aa</button>
      <button class="fb-btn {filterMode ? 'on' : ''}" title="Mostrar solo las líneas que coinciden" aria-label="Filtrar coincidencias" onclick={() => (filterMode = !filterMode)}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 5h18l-7 8v6l-4 2v-8z" /></svg>
      </button>
      <button class="fb-btn" title="Cerrar (Esc)" aria-label="Cerrar búsqueda" onclick={closeFind}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18M6 6l12 12" /></svg>
      </button>
    </div>
  {/if}
  <div class="console" tabindex="-1" bind:this={el}>{#if displayLines.length}{@html rendered.html}{:else if filterMode && query.trim()}<span class="con-empty">Sin coincidencias para “{query}”.</span>{:else}{placeholder}{/if}</div>
</div>
