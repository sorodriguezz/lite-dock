<script lang="ts">
  interface Props {
    lines: string[];
    placeholder?: string;
  }
  let { lines, placeholder = "" }: Props = $props();

  let el: HTMLDivElement | undefined = $state();

  // Auto-scroll to the bottom whenever the content changes.
  $effect(() => {
    void lines.length;
    if (el) el.scrollTop = el.scrollHeight;
  });
</script>

<div class="console" bind:this={el}>{lines.length ? lines.join("\n") : placeholder}</div>
