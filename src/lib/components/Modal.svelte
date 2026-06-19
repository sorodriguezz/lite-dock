<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    onClose: () => void;
    children?: Snippet;
    footer?: Snippet;
  }
  let { title, onClose, children, footer }: Props = $props();
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<div class="modal-backdrop" onclick={onClose} role="presentation">
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <div class="modal-head">
      <h3>{title}</h3>
      <div style="flex:1"></div>
      <button class="btn icon" onclick={onClose} aria-label="Cerrar">✕</button>
    </div>
    <div class="modal-body">
      {@render children?.()}
    </div>
    {#if footer}
      <div class="modal-foot">{@render footer()}</div>
    {/if}
  </div>
</div>
