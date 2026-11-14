<script lang="ts">
  import { thumbUrl } from "../lib/api";
  import { withFavorite } from "../lib/decisions";
  import type { LibraryStore } from "../lib/library.svelte";
  import type { Mark, Photo } from "../lib/types";

  let { store, onback, onclose }: { store: LibraryStore; onback: (id: string) => void; onclose: () => void } =
    $props();

  type Pile = "delete" | "unsure" | "approved" | "favorite";
  const piles: { key: Pile; label: string; color: string }[] = [
    { key: "delete", label: "Delete", color: "var(--del)" },
    { key: "unsure", label: "Unsure", color: "var(--uns)" },
    { key: "approved", label: "Approved", color: "var(--ok)" },
    { key: "favorite", label: "Favorites", color: "var(--fav)" },
  ];

  let pile = $state<Pile>("delete");
  let selectedId = $state<string | null>(null);
  let scrollTop = $state(0);
  let viewport = $state(600);
  let width = $state(360);

  const inPile = (photo: Photo, key: Pile) =>
    key === "favorite" ? photo.decision.favorite : photo.decision.mark === key;
  const items = $derived(store.photos.filter((photo) => inPile(photo, pile)));
  const columns = $derived(Math.max(3, Math.floor(width / 116)));
  const size = $derived(width / columns);
  const firstRow = $derived(Math.max(0, Math.floor(scrollTop / size) - 2));
  const lastRow = $derived(Math.ceil((scrollTop + viewport) / size) + 2);
  const visible = $derived(items.slice(firstRow * columns, lastRow * columns));
  const selected = $derived(selectedId ? store.photo(selectedId) : undefined);

  function setMark(photo: Photo, mark: Mark) {
    store.decide([{ id: photo.id, decision: { ...photo.decision, mark } }]);
  }

  function backToInbox(photo: Photo) {
    store.decide([{ id: photo.id, decision: { ...photo.decision, mark: null, favorite: false } }]);
    onback(photo.id);
    selectedId = null;
  }
</script>

<div class="sheet" role="dialog" aria-label="Piles">
  <header>
    <b>Piles</b>
    <span class="grow"></span>
    <button onclick={onclose}>Done</button>
  </header>
  <div class="tabs">
    {#each piles as item (item.key)}
      <button
        class:on={pile === item.key}
        onclick={() => {
          pile = item.key;
          selectedId = null;
          scrollTop = 0;
        }}
      >
        <span style:color={item.color}>●</span>
        {item.label}
        {store.photos.filter((photo) => inPile(photo, item.key)).length}
      </button>
    {/each}
  </div>
  <div
    class="grid"
    bind:clientHeight={viewport}
    bind:clientWidth={width}
    onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
  >
    <div class="spacer" style:height={`${Math.ceil(items.length / columns) * size}px`}>
      {#each visible as photo, index (photo.id)}
        {@const position = firstRow * columns + index}
        <button
          class="tile"
          class:selected={photo.id === selectedId}
          style:width={`${size}px`}
          style:height={`${size}px`}
          style:left={`${(position % columns) * size}px`}
          style:top={`${Math.floor(position / columns) * size}px`}
          onclick={() => (selectedId = photo.id)}
        >
          <img src={thumbUrl(photo)} alt={photo.name} loading="lazy" />
          {#if photo.decision.favorite}<i>♥</i>{/if}
        </button>
      {/each}
    </div>
    {#if !items.length}
      <p class="empty">Empty</p>
    {/if}
  </div>
  {#if selected}
    <div class="actions">
      <span class="name">{selected.name}</span>
      <button style:color="var(--del)" onclick={() => setMark(selected, "delete")}>Delete</button>
      <button style:color="var(--uns)" onclick={() => setMark(selected, "unsure")}>Unsure</button>
      <button style:color="var(--ok)" onclick={() => setMark(selected, "approved")}>Approve</button>
      <button
        style:color="var(--fav)"
        onclick={() => store.decide([{ id: selected.id, decision: withFavorite(selected.decision) }])}
        >{selected.decision.favorite ? "Unfavorite" : "Favorite"}</button
      >
      <button onclick={() => backToInbox(selected)}>Back to inbox</button>
    </div>
  {/if}
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
  button {
    background: #121315;
    border-color: #26282c;
    border-radius: 999px;
  }
  .tabs {
    display: flex;
    gap: 4px;
    padding: 0 14px 8px;
    overflow-x: auto;
  }
  .tabs button {
    border: 0;
    background: none;
  }
  .tabs button.on {
    background: #2a2c30;
  }
  .grid {
    flex: 1;
    overflow: auto;
    position: relative;
    margin: 0 10px;
  }
  .spacer {
    position: relative;
  }
  .tile {
    position: absolute;
    padding: 3px;
    border: 0;
    background: none;
    border-radius: 10px;
  }
  .tile img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    border-radius: 8px;
    display: block;
    border: 2px solid transparent;
  }
  .tile.selected img {
    border-color: #fff;
  }
  .tile i {
    position: absolute;
    right: 8px;
    top: 6px;
    color: var(--fav);
    font-style: normal;
  }
  .empty {
    color: #8b8e95;
    text-align: center;
  }
  .actions {
    display: flex;
    gap: 6px;
    padding: 10px 14px;
    border-top: 1px solid #26282c;
    flex-wrap: wrap;
    align-items: center;
  }
  .name {
    font: 12px var(--f-mono);
    margin-right: 6px;
  }
</style>
