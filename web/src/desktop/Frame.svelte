<script lang="ts">
  import { untrack } from "svelte";
  import { analyzeImage, type Analysis } from "../lib/analysis";
  import { previewUrl, thumbUrl } from "../lib/api";
  import { exposureLine } from "../lib/shotinfo.svelte";
  import type { Photo, ShotInfo } from "../lib/types";

  let {
    photo,
    focused,
    selected,
    solo,
    hidden,
    info,
    showFocus,
    showClipping,
    wantAnalysis,
    onanalysis,
    onfocus,
    onrate,
    onsolo,
  }: {
    photo: Photo;
    focused: boolean;
    selected: boolean;
    solo: boolean;
    hidden: boolean;
    info: ShotInfo | null | undefined;
    showFocus: boolean;
    showClipping: boolean;
    wantAnalysis: boolean;
    onanalysis: (analysis: Analysis) => void;
    onfocus: (event: MouseEvent) => void;
    onrate: (rating: number) => void;
    onsolo: () => void;
  } = $props();

  let loaded = $state(false);
  let image = $state<HTMLImageElement>();
  let picture = $state<HTMLDivElement>();
  let mask = $state<HTMLCanvasElement>();
  let analysis = $state<Analysis | null>(null);
  let box = $state({ left: 0, top: 0, width: 0, height: 0 });
  const labels = { approved: "LR approved", delete: "Delete", unsure: "Unsure" } as const;
  const details = $derived(
    info ? [exposureLine(info), info.film_simulation].filter(Boolean).join(" · ") : "",
  );
  const warning = $derived(
    info?.focus_warning ? "Camera: out of focus" : info?.blur_warning ? "Camera: blur warning" : null,
  );

  let shown = "";

  $effect(() => {
    const source = previewUrl(photo);
    untrack(() => {
      if (source === shown) return;
      shown = source;
      analysis = null;
      loaded = Boolean(image?.complete && image.naturalWidth && image.currentSrc.endsWith(source));
    });
  });

  $effect(() => {
    const element = image;
    if (!element || !loaded) return;
    const measure = () => {
      box = {
        left: element.offsetLeft,
        top: element.offsetTop,
        width: element.offsetWidth,
        height: element.offsetHeight,
      };
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    if (picture) observer.observe(picture);
    return () => observer.disconnect();
  });

  $effect(() => {
    if (!loaded || !image || analysis || !(wantAnalysis || showClipping)) return;
    const result = analyzeImage(`${photo.id}:${photo.version}`, image);
    if (result) {
      analysis = result;
      onanalysis(result);
    }
  });

  $effect(() => {
    if (!mask || !analysis || !showClipping) return;
    mask.width = analysis.width;
    mask.height = analysis.height;
    mask
      .getContext("2d")
      ?.putImageData(new ImageData(new Uint8ClampedArray(analysis.mask), analysis.width), 0, 0);
  });
</script>

<div
  class="frame"
  class:focused
  class:selected
  class:solo
  class:hidden
  class:deleted={photo.decision.mark === "delete"}
  role="button"
  tabindex="-1"
  onclick={onfocus}
  ondblclick={onsolo}
  onkeydown={() => undefined}
>
  <div class="picture" bind:this={picture}>
    {#if !loaded}
      <img class="placeholder" src={thumbUrl(photo)} alt="" />
    {/if}
    <img
      bind:this={image}
      class:ready={loaded}
      src={previewUrl(photo)}
      alt={photo.name}
      onload={() => (loaded = true)}
    />
    {#if loaded && box.width}
      <div
        class="overlay"
        style:left={`${box.left}px`}
        style:top={`${box.top}px`}
        style:width={`${box.width}px`}
        style:height={`${box.height}px`}
      >
        {#if showClipping && analysis}
          <canvas bind:this={mask} class="mask" aria-hidden="true"></canvas>
        {/if}
        {#if showFocus && info}
          {#each info.faces as face, index (index)}
            <span
              class="face"
              style:left={`${face.left * 100}%`}
              style:top={`${face.top * 100}%`}
              style:width={`${(face.right - face.left) * 100}%`}
              style:height={`${(face.bottom - face.top) * 100}%`}
            ></span>
          {/each}
          {#if info.focus_point}
            <span
              class="focus"
              class:warn={info.focus_warning === true}
              style:left={`${info.focus_point.x * 100}%`}
              style:top={`${info.focus_point.y * 100}%`}
              title={info.focus_warning ? "Focus point · the camera flagged it out of focus" : "Focus point"}
            ></span>
          {/if}
        {/if}
      </div>
    {/if}
  </div>
  <div class="labels">
    {#if photo.decision.mark}
      <span class="pill {photo.decision.mark}">{labels[photo.decision.mark]}</span>
    {/if}
    {#if photo.decision.favorite}
      <span class="pill heart" aria-label="Favorite">♥</span>
    {/if}
    {#if selected}
      <span class="pill keep">✓ keep</span>
    {/if}
    {#if photo.error}
      <span class="pill delete" title={photo.error}>Unreadable</span>
    {/if}
    {#if warning}
      <span class="pill warn">{warning}</span>
    {/if}
  </div>
  <div class="stars">
    {#each [1, 2, 3, 4, 5] as star (star)}
      <button
        class:on={star <= photo.decision.rating}
        aria-label={`${star} star`}
        onclick={(event) => {
          event.stopPropagation();
          onrate(star);
        }}>★</button
      >
    {/each}
  </div>
  <div class="name">
    {photo.name}{#if details}<span class="details"> · {details}</span>{/if}
  </div>
</div>

<style>
  .frame {
    position: relative;
    flex: 1 1 0;
    min-width: 0;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    outline: none;
  }
  .frame.solo {
    flex: 1 1 100%;
  }
  .frame.hidden {
    display: none;
  }
  .picture {
    position: relative;
    max-width: 100%;
    max-height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    width: 100%;
  }
  .picture img {
    max-width: 100%;
    max-height: calc(100% - 26px);
    object-fit: contain;
    border-radius: 2px;
    box-shadow: 0 2px 10px #0004;
    outline: 3px solid transparent;
    outline-offset: 3px;
  }
  .picture img:not(.placeholder):not(.ready) {
    position: absolute;
    opacity: 0;
  }
  .placeholder {
    filter: blur(1px);
  }
  .focused .picture img {
    outline-color: #fff;
  }
  .selected .picture img {
    outline-color: var(--ok);
  }
  .deleted .picture img {
    opacity: 0.35;
  }
  .labels {
    position: absolute;
    left: 8px;
    top: 8px;
    display: flex;
    gap: 5px;
  }
  .pill {
    font: 600 11px var(--f-body);
    padding: 2px 8px;
    border-radius: 999px;
    color: #fff;
  }
  .pill.approved {
    background: var(--ok);
  }
  .pill.delete {
    background: var(--del);
  }
  .pill.unsure {
    background: var(--uns);
  }
  .pill.heart {
    background: #fff;
    color: var(--heart);
  }
  .pill.warn {
    background: #b4410f;
  }
  .overlay {
    position: absolute;
    pointer-events: none;
  }
  .mask {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  .focus {
    position: absolute;
    width: 44px;
    height: 44px;
    margin: -22px 0 0 -22px;
    border: 2px solid #2fbf71;
    border-radius: 3px;
    box-shadow:
      0 0 0 1px #0008,
      inset 0 0 0 1px #0006;
  }
  .focus.warn {
    border-color: #f08a24;
  }
  .face {
    position: absolute;
    border: 1px dashed #ffffffd9;
    border-radius: 3px;
    box-shadow: 0 0 0 1px #0005;
  }
  .details {
    opacity: 0.85;
  }
  .pill.keep {
    background: #fff;
    color: var(--ok);
  }
  .stars {
    position: absolute;
    left: 50%;
    bottom: 22px;
    transform: translateX(-50%);
    display: flex;
    gap: 2px;
    background: #0007;
    border-radius: 999px;
    padding: 0 8px;
  }
  .stars button {
    border: 0;
    background: none;
    color: #ffffff55;
    font-size: 16px;
    padding: 0 1px;
  }
  .stars button.on {
    color: var(--fav);
  }
  .name {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    text-align: center;
    font: 11px var(--f-mono);
    color: #f4f4f2;
    text-shadow: 0 1px 2px #0008;
  }
</style>
