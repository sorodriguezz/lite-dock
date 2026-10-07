<script lang="ts">
  import { toasts, dismissToast } from "../stores";
</script>

<div class="toasts" aria-live="polite">
  {#each $toasts as t (t.id)}
    <div class="toast {t.kind}" role={t.kind === "error" ? "alert" : "status"}>
      <span class="toast-text">{t.text}</span>
      <button class="toast-x" aria-label="Cerrar aviso" title="Cerrar" onclick={() => dismissToast(t.id)}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg>
      </button>
    </div>
  {/each}
</div>

<style>
  .toast {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .toast-text {
    flex: 1;
    min-width: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 40vh;
    overflow: auto;
    user-select: text;
  }
  .toast-x {
    flex: none;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    margin: -3px -6px -3px 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .toast-x:hover {
    background: var(--panel-2);
    color: var(--text);
  }
  .toast-x svg {
    width: 13px;
    height: 13px;
  }
</style>
