<script lang="ts">
  import { thumbUrl } from "../lib/api";
  import type { Scene } from "../lib/grouping";
  import ProgressBar from "./ProgressBar.svelte";
  import { clock, day, tally } from "./progress";

  let { scenes, current, onpick }: { scenes: Scene[]; current: number; onpick: (index: number) => void } =
    $props();

  let list: HTMLElement | undefined = $state();

  $effect(() => {
    list?.querySelector(`[data-scene="${current}"]`)?.scrollIntoView({ block: "nearest" });
  });
</script>

<nav bind:this={list} aria-label="Scenes">
  <h3>SCENES</h3>
  {#each scenes as scene, index (scene.photos[0]?.id ?? index)}
    {@const first = scene.photos[0]}
    {@const left = tally(scene.photos).undecided}
    <button class="scene" class:current={index === current} data-scene={index} onclick={() => onpick(index)}>
      <div class="top">
        <b>{first ? `${day(first.time_ms)} · ${clock(first.time_ms, false)}` : `Scene ${index + 1}`}</b>
        <span>{scene.photos.length}</span>
      </div>
      <div class="mosaic">
        {#each scene.stacks.slice(0, 8) as stack (stack[0]?.id)}
          {#if stack[0]}
            <img src={thumbUrl(stack[0])} alt="" loading="lazy" />
          {/if}
        {/each}
      </div>
      <ProgressBar photos={scene.photos} />
      <div class="sub">{scene.stacks.length} stacks · {left ? `${left} to decide` : "done ✓"}</div>
    </button>
  {/each}
</nav>

<style>
  nav {
    width: 250px;
    border-right: 1px solid var(--line);
    overflow: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex-shrink: 0;
  }
  h3 {
    margin: 2px 4px;
    font: 600 11px var(--f-mono);
    letter-spacing: 0.08em;
    color: var(--mut);
  }
  .scene {
    background: var(--panel);
    border-radius: 10px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    text-align: left;
    white-space: normal;
  }
  .scene.current {
    border-color: var(--acc);
    box-shadow: 0 0 0 1px var(--acc);
  }
  .top {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
  }
  .mosaic {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 2px;
    border-radius: 6px;
    overflow: hidden;
  }
  .mosaic img {
    width: 100%;
    aspect-ratio: 3 / 2;
    object-fit: cover;
    display: block;
    background: var(--line);
  }
  .sub {
    font: 11px var(--f-mono);
    color: var(--mut);
  }
</style>
