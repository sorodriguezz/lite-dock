<script lang="ts">
  import { onMount, type Snippet } from "svelte";

  interface Props {
    title: string;
    onClose: () => void;
    /** Close with Escape. Off by default for big forms/details, on for confirmations. */
    closeOnEsc?: boolean;
    children?: Snippet;
    footer?: Snippet;
  }
  let { title, onClose, closeOnEsc = false, children, footer }: Props = $props();

  let dialog = $state<HTMLDivElement>();
  const titleId = `modal-title-${Math.random().toString(36).slice(2, 8)}`;

  const FOCUSABLE =
    'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  onMount(() => {
    // Move focus into the dialog (so Enter can't re-trigger the button behind it)
    // and give it back to whatever opened the dialog when it closes.
    const opener = document.activeElement as HTMLElement | null;
    const auto = dialog?.querySelector<HTMLElement>("[data-autofocus]");
    (auto ?? dialog)?.focus();
    return () => opener?.focus?.();
  });

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && closeOnEsc) {
      e.stopPropagation();
      onClose();
      return;
    }
    // Keep Tab cycling inside the dialog.
    if (e.key === "Tab" && dialog) {
      const items = [...dialog.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null);
      if (!items.length) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (e.shiftKey && (document.activeElement === first || document.activeElement === dialog)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }
</script>

<!-- Closes via the ✕ / footer buttons (and Escape when `closeOnEsc`) — clicking
     the backdrop does not dismiss it, to avoid losing what you were doing. -->
<div class="modal-backdrop">
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby={titleId}
    tabindex="-1"
    bind:this={dialog}
    onkeydown={onKeydown}
  >
    <div class="modal-head">
      <h3 id={titleId}>{title}</h3>
      <div style="flex:1"></div>
      <button class="btn icon" onclick={onClose} aria-label="Cerrar" title="Cerrar">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg>
      </button>
    </div>
    <div class="modal-body">
      {@render children?.()}
    </div>
    {#if footer}
      <div class="modal-foot">{@render footer()}</div>
    {/if}
  </div>
</div>
