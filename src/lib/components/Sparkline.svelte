<script lang="ts">
  // Small live chart: a Y axis (left) with max on top and 0 at the bottom, an X
  // axis (bottom) with the time span, and the line plotted INSIDE the axes so the
  // labels never overlap it.
  interface Props {
    data: number[];
    max?: number; // fixed Y scale; if omitted, auto-scales to the data peak
    color?: string;
    height?: number;
    peakLabel?: string; // Y-axis top label (the scale / peak)
    spanLabel?: string; // X-axis label (the time window)
  }
  let {
    data,
    max,
    color = "var(--accent)",
    height = 66,
    peakLabel = "",
    spanLabel = "",
  }: Props = $props();

  const W = 240;

  let geom = $derived.by(() => {
    if (data.length < 2) return { line: "", area: "" };
    const m = max ?? Math.max(1, ...data) * 1.1;
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

<div class="chart" style="--chart-h:{height}px">
  <div class="chart-y">
    <span>{peakLabel}</span>
    <span>0</span>
  </div>
  <div class="chart-plot">
    <svg viewBox="0 0 {W} {height}" preserveAspectRatio="none" aria-hidden="true">
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
  </div>
  {#if spanLabel}<div class="chart-x">{spanLabel}</div>{/if}
</div>
