<script lang="ts">
  import HistogramChart from "../components/HistogramChart.svelte";
  import { percent, type Analysis } from "../lib/analysis";
  import { exposureBias, exposureLine, focalLine } from "../lib/shotinfo.svelte";
  import type { Photo, ShotInfo } from "../lib/types";

  let {
    photo,
    info,
    analysis,
    onrate,
    onclose,
  }: {
    photo: Photo;
    info: ShotInfo | null | undefined;
    analysis: Analysis | null;
    onrate: (rating: number) => void;
    onclose: () => void;
  } = $props();

  const verdict = $derived(
    info?.focus_warning === true
      ? { text: "Out of focus", tone: "warn" }
      : info?.blur_warning === true
        ? { text: "Blur warning", tone: "warn" }
        : info?.focus_warning === false
          ? { text: "In focus", tone: "ok" }
          : null,
  );
  const gear = $derived([info?.camera, info?.lens].filter(Boolean).join(" · "));
  const exposure = $derived(info ? exposureLine(info) : "");
  const mode = $derived(
    [info ? exposureBias(info.exposure_bias) : null, info?.program].filter(Boolean).join(" · "),
  );
  const tone = $derived(
    [info?.dynamic_range, info?.grain ? `grain ${info.grain.toLowerCase()}` : null]
      .filter(Boolean)
      .join(" · "),
  );
  const focus = $derived(
    [info?.focus_mode, info?.faces.length ? "face detected" : null].filter(Boolean).join(" · "),
  );
  const focal = $derived(info ? focalLine(info) : null);
</script>

<section class="sheet" aria-label="Shot info">
  <button class="grab" onclick={onclose} aria-label="Hide shot info"></button>
  <div class="head">
    <b>{photo.name.replace(/\.[^.]+$/, "")}</b>
    <span class="gear">{gear}</span>
    <span class="grow"></span>
    {#if verdict}<span class={verdict.tone}>{verdict.text}</span>{/if}
  </div>
  <HistogramChart {analysis} color={true} height={64} />
  {#if analysis}
    <div class="clip">
      <span>Shadows {percent(analysis.shadows)}</span>
      <span class="highlights">Highlights {percent(analysis.highlights)}</span>
    </div>
  {/if}
  {#if info}
    <dl>
      {#if exposure}<div class="wide">
          <dt>Exposure</dt>
          <dd class="mono">{exposure}</dd>
        </div>{/if}
      {#if mode}<div>
          <dt>Mode</dt>
          <dd>{mode}</dd>
        </div>{/if}
      {#if focal}<div>
          <dt>Focal length</dt>
          <dd>{focal}</dd>
        </div>{/if}
      {#if tone}<div>
          <dt>Tone</dt>
          <dd>{tone}</dd>
        </div>{/if}
      {#if focus}<div>
          <dt>Focus</dt>
          <dd>{focus}</dd>
        </div>{/if}
    </dl>
  {:else if info === null}
    <p class="muted">No shot info in this file.</p>
  {/if}
  <div class="stars" role="group" aria-label="Rating">
    {#each [1, 2, 3, 4, 5] as star (star)}
      <button
        class:on={star <= photo.decision.rating}
        aria-label={`${star} star${star > 1 ? "s" : ""}`}
        onclick={() => onrate(star === photo.decision.rating ? 0 : star)}>★</button
      >
    {/each}
  </div>
</section>

<style>
  .sheet {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 5;
    background: #141518;
    border: 1px solid #2a2c31;
    border-radius: 18px;
    padding: 6px 14px 12px;
    display: flex;
    flex-direction: column;
    gap: 9px;
    color: #f2f2f3;
    --histogram-bg: #1d1f23;
    --histogram-line: #c9cbd1;
    --histogram-luma: #c9cbd155;
  }
  .grab {
    align-self: center;
    width: 64px;
    height: 18px;
    border: 0;
    background: none;
    position: relative;
  }
  .grab::after {
    content: "";
    position: absolute;
    left: 12px;
    right: 12px;
    top: 7px;
    height: 4px;
    border-radius: 999px;
    background: #3a3c42;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .gear {
    font: 11px var(--f-mono);
    color: #a6a8ae;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .grow {
    flex: 1;
  }
  .ok {
    color: #6fdc9f;
    font-weight: 600;
    font-size: 12px;
  }
  .warn {
    color: #ffa45c;
    font-weight: 600;
    font-size: 12px;
  }
  .clip {
    display: flex;
    justify-content: space-between;
    font: 11px var(--f-mono);
    color: #a6a8ae;
  }
  .highlights {
    color: #ff6fa8;
  }
  dl {
    margin: 0;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px 12px;
    font-size: 12px;
  }
  dt {
    color: #a6a8ae;
    font-size: 11px;
  }
  dd {
    margin: 0;
  }
  .wide {
    grid-column: 1 / -1;
  }
  .mono {
    font: 500 13px var(--f-mono);
  }
  .muted {
    margin: 0;
    color: #a6a8ae;
    font-size: 12px;
  }
  .stars {
    display: flex;
    justify-content: center;
    gap: 6px;
  }
  .stars button {
    width: 44px;
    height: 40px;
    border-radius: 10px;
    background: #1d1f23;
    border: 1px solid #2a2c31;
    color: #5b5e66;
    font-size: 17px;
  }
  .stars button.on {
    color: var(--fav);
  }
</style>
