<script lang="ts">
  import { confirmReq } from "../stores";
  import Modal from "./Modal.svelte";

  function answer(ok: boolean) {
    const r = $confirmReq;
    confirmReq.set(null);
    r?.resolve(ok);
  }
</script>

{#if $confirmReq}
  {@const req = $confirmReq}
  <Modal title={req.title} onClose={() => answer(false)}>
    <p class="confirm-msg">{req.message}</p>
    {#snippet footer()}
      <button class="btn" onclick={() => answer(false)}>No</button>
      <button class="btn {req.danger ? 'danger' : 'primary'}" onclick={() => answer(true)}>
        {req.confirmText}
      </button>
    {/snippet}
  </Modal>
{/if}

<style>
  .confirm-msg {
    margin: 0;
    color: var(--muted);
    line-height: 1.55;
    font-size: 14px;
  }
</style>
