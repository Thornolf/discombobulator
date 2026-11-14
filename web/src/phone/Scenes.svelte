<script lang="ts">
  import { thumbUrl } from "../lib/api";
  import type { Scene } from "../lib/grouping";
  import ProgressBar from "../desktop/ProgressBar.svelte";
  import { clock, day } from "../desktop/progress";
  import { waitingInScene, type DeckMode } from "./swipe";

  let {
    scenes,
    mode,
    current,
    onpick,
    onclose,
  }: {
    scenes: Scene[];
    mode: DeckMode;
    current: number;
    onpick: (index: number) => void;
    onclose: () => void;
  } = $props();

  let list: HTMLElement | undefined = $state();

  $effect(() => {
    list?.querySelector(`[data-scene="${current}"]`)?.scrollIntoView({ block: "center" });
  });
</script>

<div class="sheet" role="dialog" aria-label="Scenes">
  <header>
    <b>Scenes</b>
    <span class="grow"></span>
    <button onclick={onclose}>Done</button>
  </header>
  <div class="list" bind:this={list}>
    {#each scenes as scene, index (scene.photos[0]?.id ?? index)}
      {@const first = scene.photos[0]}
      {@const left = waitingInScene(scene.photos, mode).size}
      <button
        class="scene"
        class:current={index === current}
        data-scene={index}
        onclick={() => onpick(index)}
      >
        <div class="top">
          <b>{first ? `${day(first.time_ms)} · ${clock(first.time_ms, false)}` : `Scene ${index + 1}`}</b>
          <span>{scene.photos.length} photos</span>
        </div>
        <div class="mosaic">
          {#each scene.stacks.slice(0, 8) as stack (stack[0]?.id)}
            {#if stack[0]}
              <img src={thumbUrl(stack[0])} alt="" loading="lazy" />
            {/if}
          {/each}
        </div>
        <ProgressBar photos={scene.photos} />
        <div class="sub">{left ? `${left} ${mode === "inbox" ? "to decide" : "unsure"}` : "done ✓"}</div>
      </button>
    {/each}
    {#if !scenes.length}
      <p class="empty">No photos</p>
    {/if}
  </div>
</div>

<style>
  .sheet {
    position: fixed;
    inset: 0;
    background: #000;
    color: #f2f2f3;
    z-index: 40;
    display: flex;
    flex-direction: column;
    padding-top: env(safe-area-inset-top);
    padding-bottom: env(safe-area-inset-bottom);
    color-scheme: dark;
  }
  header {
    display: flex;
    align-items: center;
    padding: 12px 14px;
  }
  .grow {
    flex: 1;
  }
  header button {
    background: #121315;
    border-color: #26282c;
    border-radius: 999px;
  }
  .list {
    flex: 1;
    overflow: auto;
    padding: 0 14px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .scene {
    background: #121315;
    border: 1px solid #26282c;
    border-radius: 12px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    text-align: left;
    white-space: normal;
    color: inherit;
  }
  .scene.current {
    border-color: var(--acc);
    box-shadow: 0 0 0 1px var(--acc);
  }
  .top {
    display: flex;
    justify-content: space-between;
    font-size: 14px;
  }
  .top span,
  .sub {
    font: 12px var(--f-mono);
    color: #8b8d93;
  }
  .mosaic {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 2px;
    border-radius: 8px;
    overflow: hidden;
  }
  .mosaic img {
    width: 100%;
    aspect-ratio: 3 / 2;
    object-fit: cover;
    display: block;
    background: #26282c;
  }
  .empty {
    color: #8b8d93;
    text-align: center;
  }
</style>
