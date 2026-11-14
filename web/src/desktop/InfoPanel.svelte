<script lang="ts">
  import HistogramChart from "../components/HistogramChart.svelte";
  import { percent, type Analysis } from "../lib/analysis";
  import { describe, type Action, type Keymap } from "../lib/keymap";
  import { exposureBias, exposureLine, focalLine } from "../lib/shotinfo.svelte";
  import type { Photo, ShotInfo } from "../lib/types";
  import type { ViewOptions } from "../lib/view.svelte";

  let {
    photo,
    info,
    analysis,
    view,
    keymap,
    position,
    ontoggle,
  }: {
    photo: Photo;
    info: ShotInfo | null | undefined;
    analysis: Analysis | null;
    view: ViewOptions;
    keymap: Keymap;
    position: string | null;
    ontoggle: (action: Action) => void;
  } = $props();

  const keys = (action: Action) => keymap[action].map(describe).join(" ");
  const verdict = $derived(
    info?.focus_warning === true
      ? { text: "Camera: out of focus", tone: "warn" }
      : info?.blur_warning === true
        ? { text: "Camera: blur warning", tone: "warn" }
        : info?.focus_warning === false
          ? { text: "Camera: in focus", tone: "ok" }
          : null,
  );
  const tone = $derived(
    [info?.dynamic_range, info?.grain ? `grain ${info.grain.toLowerCase()}` : null]
      .filter(Boolean)
      .join(" · "),
  );
  const focus = $derived(
    [info?.focus_mode, info?.af_area, info?.faces.length ? "face detected" : null]
      .filter(Boolean)
      .join(" · "),
  );
  const exposure = $derived(info ? exposureLine(info) : "");
  const compensation = $derived(
    [info ? exposureBias(info.exposure_bias) : null, info?.program].filter(Boolean).join(" · "),
  );
  const focal = $derived(info ? focalLine(info) : null);
  const lens = $derived([info?.lens, focal].filter(Boolean).join(" · "));
</script>

<aside class="info" aria-label="Shot info">
  <div class="head">
    <h2>{photo.name.replace(/\.[^.]+$/, "")}</h2>
    <span class="sub"
      >{[photo.raf && photo.jpg ? "RAF + JPG" : photo.raf ? "RAF" : "JPG", position]
        .filter(Boolean)
        .join(" · ")}</span
    >
    <span class="grow"></span>
    <button class="hide" onclick={() => ontoggle("info")}>Hide <kbd>{keys("info")}</kbd></button>
  </div>

  {#if view.histogram}
    <div class="card">
      <div class="card-head">
        <b>Histogram</b>
        <span class="grow"></span>
        <button class="mode" onclick={() => ontoggle("histogramMode")}>
          {view.colorHistogram ? "colors" : "brightness"} <kbd>{keys("histogramMode")}</kbd>
        </button>
      </div>
      <HistogramChart {analysis} color={view.colorHistogram} />
      {#if analysis}
        <div class="clip">
          <span class="shadows">Shadows {percent(analysis.shadows)}</span>
          <span class="highlights">Highlights {percent(analysis.highlights)}</span>
        </div>
      {/if}
    </div>
  {/if}

  {#if info === undefined}
    <p class="muted">Reading shot info…</p>
  {:else if info === null}
    <p class="muted">No shot info in this file.</p>
  {:else}
    <dl>
      {#if exposure}<dt>Exposure</dt>
        <dd class="mono">{exposure}</dd>{/if}
      {#if compensation}<dt>Compensation</dt>
        <dd class="mono">{compensation}</dd>{/if}
      {#if lens}<dt>Lens</dt>
        <dd>{lens}</dd>{/if}
      {#if info.camera}<dt>Body</dt>
        <dd>{info.camera}</dd>{/if}
      {#if info.film_simulation}<dt>Film simulation</dt>
        <dd><span class="film">{info.film_simulation}</span></dd>{/if}
      {#if tone}<dt>Tone</dt>
        <dd>{tone}</dd>{/if}
      {#if focus}<dt>Focus</dt>
        <dd>{focus}</dd>{/if}
      {#if verdict}<dt>Camera check</dt>
        <dd class={verdict.tone}>{verdict.text}</dd>{/if}
      {#if info.exposure_warning === true}<dt>Exposure check</dt>
        <dd class="warn">Camera: bad exposure</dd>{/if}
    </dl>
  {/if}

  <div class="keys">
    <button class:on={view.info} onclick={() => ontoggle("info")}><kbd>{keys("info")}</kbd> panel</button>
    <button class:on={view.focusPoint} aria-pressed={view.focusPoint} onclick={() => ontoggle("focusPoint")}
      ><kbd>{keys("focusPoint")}</kbd> focus point</button
    >
    <button class:on={view.histogram} aria-pressed={view.histogram} onclick={() => ontoggle("histogram")}
      ><kbd>{keys("histogram")}</kbd> histogram</button
    >
    <button class:on={view.clipping} aria-pressed={view.clipping} onclick={() => ontoggle("clipping")}
      ><kbd>{keys("clipping")}</kbd> clipping</button
    >
  </div>
</aside>

<style>
  .info {
    width: 300px;
    flex-shrink: 0;
    border-left: 1px solid var(--line);
    background: var(--panel);
    padding: 12px 14px 14px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    overflow: auto;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  h2 {
    margin: 0;
    font-size: 14px;
  }
  .sub,
  .muted {
    font: 11px var(--f-mono);
    color: var(--mut);
  }
  .grow {
    flex: 1;
  }
  .hide,
  .mode {
    border: 0;
    background: none;
    color: var(--mut);
    padding: 2px 4px;
    font-size: 11px;
  }
  .card {
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .card-head {
    display: flex;
    align-items: center;
    font-size: 12px;
  }
  .clip {
    display: flex;
    justify-content: space-between;
    font: 11px var(--f-mono);
    color: var(--mut);
  }
  .highlights {
    color: #c0266d;
  }
  .shadows {
    color: #2f6fd8;
  }
  dl {
    margin: 0;
    display: grid;
    grid-template-columns: 104px minmax(0, 1fr);
    gap: 8px 10px;
    font-size: 12px;
  }
  dt {
    color: var(--mut);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .mono {
    font: 500 12px var(--f-mono);
  }
  .film {
    background: #f3ead8;
    color: #5f430f;
    border-radius: 4px;
    padding: 1px 6px;
    font-weight: 600;
  }
  .ok {
    color: var(--ok);
    font-weight: 600;
  }
  .warn {
    color: #c2410c;
    font-weight: 600;
  }
  .keys {
    margin-top: auto;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .keys button {
    font-size: 11px;
    padding: 3px 6px;
    color: var(--mut);
  }
  .keys button.on {
    color: var(--tx);
    border-color: var(--acc);
  }
</style>
