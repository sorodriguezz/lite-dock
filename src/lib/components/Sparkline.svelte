<script lang="ts">
  // Real-time line chart for a rolling array of samples, with axis labels:
  // peak (Y max) top-right, 0 bottom-left, and a time span bottom-right.
  interface Props {
    data: number[];
    max?: number; // fixed Y scale; if omitted, auto-scales to the data peak
    color?: string;
    height?: number;
    peakLabel?: string; // text shown at the top (the Y scale)
    spanLabel?: string; // text shown bottom-right (the X span)
  }
  let {
    data,
    max,
    color = "var(--accent)",
    height = 64,
    peakLabel = "",
    spanLabel = "",
  }: Props = $props();

  const W = 240;

  let geom = $derived.by(() => {
    if (data.length < 2) return { line: "", area: "" };
    const m = max ?? Math.max(1, ...data);
    const stepX = W / (data.length - 1);
    const pts = data.map((v, i) => {
      const x = i * stepX;
      const y = height - (Math.min(Math.max(v, 0), m) / m) * height;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    });
    const line = pts.join(" ");
    return { line, area: `0,${height} ${line} ${W},${height}` };
  });
</script>

<div class="spark-wrap" style="height:{height}px">
  <svg class="spark" viewBox="0 0 {W} {height}" preserveAspectRatio="none" aria-hidden="true">
    {#if geom.line}
      <polygon points={geom.area} fill={color} fill-opacity="0.13" />
      <polyline
        points={geom.line}
        fill="none"
        stroke={color}
        stroke-width="2"
        vector-effect="non-scaling-stroke"
        stroke-linejoin="round"
      />
    {/if}
  </svg>
  {#if peakLabel}<span class="spark-y">{peakLabel}</span>{/if}
  <span class="spark-zero">0</span>
  {#if spanLabel}<span class="spark-x">{spanLabel}</span>{/if}
</div>
