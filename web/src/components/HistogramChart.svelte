<script lang="ts">
  import { histogramPath, type Analysis } from "../lib/analysis";

  let {
    analysis,
    color,
    height = 96,
  }: { analysis: Analysis | null; color: boolean; height?: number } = $props();

  const width = 320;
  const paths = $derived(
    analysis
      ? {
          red: histogramPath(analysis.histogram.red, width, height),
          green: histogramPath(analysis.histogram.green, width, height),
          blue: histogramPath(analysis.histogram.blue, width, height),
          luma: histogramPath(analysis.histogram.luma, width, height),
        }
      : null,
  );
</script>

<svg
  class="histogram"
  viewBox={`0 0 ${width} ${height}`}
  preserveAspectRatio="none"
  role="img"
  aria-label={color ? "Color histogram" : "Brightness histogram"}
  style:height={`${height}px`}
>
  {#if paths}
    {#if color}
      <path d={paths.red} class="red" />
      <path d={paths.green} class="green" />
      <path d={paths.blue} class="blue" />
      <path d={paths.luma} class="luma outline" />
    {:else}
      <path d={paths.luma} class="luma" />
    {/if}
  {/if}
</svg>

<style>
  .histogram {
    width: 100%;
    display: block;
    background: var(--histogram-bg, color-mix(in srgb, var(--line) 45%, transparent));
    border-radius: 6px;
  }
  .red {
    fill: #e2404066;
  }
  .green {
    fill: #2ea05a66;
  }
  .blue {
    fill: #3c6ee666;
  }
  .luma {
    fill: var(--histogram-luma, color-mix(in srgb, var(--mut) 55%, transparent));
  }
  .luma.outline {
    fill: none;
    stroke: var(--histogram-line, var(--mut));
    stroke-width: 1;
  }
</style>
