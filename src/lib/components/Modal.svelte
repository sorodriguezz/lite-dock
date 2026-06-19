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

<!-- Closes ONLY via the ✕ / footer buttons — clicking the backdrop (or Esc)
     does not dismiss it, to avoid losing what you were doing by accident. -->
<div class="modal-backdrop">
  <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
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
