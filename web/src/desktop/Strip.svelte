<script lang="ts">
  import { thumbUrl } from "../lib/api";
  import type { Stack } from "../lib/grouping";

  let { stacks, current, onpick }: { stacks: Stack[]; current: number; onpick: (index: number) => void } =
    $props();

  let strip: HTMLElement | undefined = $state();

  $effect(() => {
    strip
      ?.querySelector(`[data-stack="${current}"]`)
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  });
</script>

<div class="strip" bind:this={strip}>
  {#each stacks as stack, index (stack[0]?.id ?? index)}
    {@const best = Math.max(0, ...stack.map((photo) => photo.decision.rating))}
    <button
      class="stack"
      class:current={index === current}
      class:done={stack.every((photo) => photo.decision.mark)}
      data-stack={index}
      onclick={() => onpick(index)}
    >
      {#if stack[0]}
        <img src={thumbUrl(stack[0])} alt="" loading="lazy" />
      {/if}
      {#if stack.length > 1}
        <span class="count">×{stack.length}</span>
      {/if}
      {#if best}
        <span class="best">{"★".repeat(best)}</span>
      {/if}
      <div class="dots">
        {#each stack as photo (photo.id)}
          <i class={photo.decision.mark ?? ""}></i>
        {/each}
      </div>
    </button>
  {/each}
</div>

<style>
  .strip {
    display: flex;
    gap: 8px;
    overflow-x: auto;
    padding: 10px 14px;
    border-top: 1px solid var(--line);
    background: var(--panel);
    flex-shrink: 0;
  }
  .stack {
    position: relative;
    width: 104px;
    flex-shrink: 0;
    border-radius: 7px;
    border: 2px solid transparent;
    padding: 2px;
    background: none;
  }
  .stack.current {
    border-color: var(--acc);
  }
  .stack.done {
    opacity: 0.55;
  }
  img {
    width: 100%;
    aspect-ratio: 3 / 2;
    object-fit: cover;
    display: block;
    border-radius: 5px;
    background: var(--line);
  }
  .count,
  .best {
    position: absolute;
    top: 6px;
    font: 600 10px var(--f-mono);
    background: #000a;
    border-radius: 4px;
    padding: 0 4px;
  }
  .count {
    right: 6px;
    color: #fff;
  }
  .best {
    left: 6px;
    color: var(--fav);
  }
  .dots {
    display: flex;
    gap: 2px;
    margin-top: 4px;
    height: 4px;
  }
  .dots i {
    flex: 1;
    border-radius: 2px;
    background: var(--line);
  }
  .dots i.approved {
    background: var(--ok);
  }
  .dots i.unsure {
    background: var(--uns);
  }
  .dots i.delete {
    background: var(--del);
  }
</style>
