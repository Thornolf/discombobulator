<script lang="ts">
  import { previewUrl, thumbUrl } from "../lib/api";
  import type { Photo } from "../lib/types";

  let {
    photo,
    focused,
    selected,
    solo,
    hidden,
    onfocus,
    onrate,
    onsolo,
  }: {
    photo: Photo;
    focused: boolean;
    selected: boolean;
    solo: boolean;
    hidden: boolean;
    onfocus: (event: MouseEvent) => void;
    onrate: (rating: number) => void;
    onsolo: () => void;
  } = $props();

  let loaded = $state(false);
  const labels = { approved: "LR approved", delete: "Delete", unsure: "Unsure" } as const;

  $effect(() => {
    void photo.id;
    loaded = false;
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
  <div class="picture">
    {#if !loaded}
      <img class="placeholder" src={thumbUrl(photo)} alt="" />
    {/if}
    <img class:ready={loaded} src={previewUrl(photo)} alt={photo.name} onload={() => (loaded = true)} />
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
  <div class="name">{photo.name}</div>
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
