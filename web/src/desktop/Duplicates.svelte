<script lang="ts">
  import { api, thumbUrl } from "../lib/api";
  import { keepPlan, withFavorite, withMark, type MarkAction } from "../lib/decisions";
  import type { LibraryStore } from "../lib/library.svelte";
  import type { DuplicateGroup, Photo } from "../lib/types";

  let { store, onopen, onclose }: { store: LibraryStore; onopen: (id: string) => void; onclose: () => void } =
    $props();

  let tolerance = $state(6);
  let groups = $state<DuplicateGroup[] | null>(null);
  let error = $state<string | null>(null);
  let groupIndex = $state(0);
  let photoIndex = $state(0);

  const resolved = $derived(
    (groups ?? []).map((group) => ({
      kind: group.kind,
      photos: group.ids.map((id) => store.photo(id)).filter((photo): photo is Photo => photo !== undefined),
    })),
  );
  const current = $derived(resolved[groupIndex]?.photos[photoIndex]);
  const exactCount = $derived(resolved.filter((group) => group.kind === "exact").length);

  let request = 0;

  async function load(value: number) {
    const ticket = ++request;
    error = null;
    try {
      const result = await api.duplicates(value);
      if (ticket !== request) return;
      groups = result;
      groupIndex = 0;
      photoIndex = 0;
    } catch (failure) {
      if (ticket === request) error = failure instanceof Error ? failure.message : String(failure);
    }
  }

  $effect(() => {
    const value = tolerance;
    const timer = setTimeout(() => void load(value), 200);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    document
      .querySelector(`[data-group="${groupIndex}"] [data-photo="${photoIndex}"]`)
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  });

  function mark(action: MarkAction) {
    if (current) store.decide([{ id: current.id, decision: withMark(current.decision, action) }]);
  }

  function keepOne() {
    const group = resolved[groupIndex];
    if (!group || !current) return;
    const plan = keepPlan(group.photos, new Set(), current.id);
    store.decide([
      ...plan.approve.flatMap((id) => {
        const photo = store.photo(id);
        return photo ? [{ id, decision: { ...photo.decision, mark: "approved" as const } }] : [];
      }),
      ...plan.remove.flatMap((id) => {
        const photo = store.photo(id);
        return photo ? [{ id, decision: { ...photo.decision, mark: "delete" as const } }] : [];
      }),
    ]);
    if (groupIndex < resolved.length - 1) {
      groupIndex += 1;
      photoIndex = 0;
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.target instanceof HTMLInputElement) return;
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") {
      event.preventDefault();
      event.stopPropagation();
      store.undo();
      return;
    }
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
    const group = resolved[groupIndex];
    const handled = (() => {
      switch (key) {
        case "ArrowRight":
          photoIndex = Math.min(photoIndex + 1, (group?.photos.length ?? 1) - 1);
          return true;
        case "ArrowLeft":
          photoIndex = Math.max(0, photoIndex - 1);
          return true;
        case "ArrowDown":
          if (groupIndex < resolved.length - 1) {
            groupIndex += 1;
            photoIndex = 0;
          }
          return true;
        case "ArrowUp":
          if (groupIndex > 0) {
            groupIndex -= 1;
            photoIndex = 0;
          }
          return true;
        case "k":
          keepOne();
          return true;
        case "x":
        case "Delete":
          mark("delete");
          return true;
        case "u":
          mark("unsure");
          return true;
        case "a":
          mark("approved");
          return true;
        case "f":
          if (current) store.decide([{ id: current.id, decision: withFavorite(current.decision) }]);
          return true;
        case "Enter":
          if (current) onopen(current.id);
          return true;
        case "Escape":
        case "d":
          onclose();
          return true;
        default:
          return false;
      }
    })();
    if (handled) {
      event.preventDefault();
      event.stopPropagation();
    }
  }
</script>

<svelte:window onkeydowncapture={onkeydown} />

<div class="view" role="dialog" aria-label="Duplicates">
  <header>
    <h2>Duplicates</h2>
    {#if groups}
      <span><span class="pill exact">EXACT</span> {exactCount} groups</span>
      <span><span class="pill similar">SIMILAR</span> {resolved.length - exactCount} groups</span>
    {/if}
    <span class="grow"></span>
    <label
      >Similarity tolerance <input type="range" min="0" max="16" bind:value={tolerance} />
      {tolerance}/64</label
    >
    <button onclick={onclose}>Close <kbd>Esc</kbd></button>
  </header>
  <p class="help">
    Exact = same bytes. Similar = perceptual hash within the tolerance. <kbd>↑</kbd><kbd>↓</kbd> group,
    <kbd>←</kbd><kbd>→</kbd> photo, <kbd>K</kbd> keep this one and mark the undecided rest Delete,
    <kbd>Enter</kbd> open in Stacks.
  </p>
  {#if error}
    <p class="error">{error}</p>
  {:else if !groups}
    <p class="help">Hashing files…</p>
  {:else if !resolved.length}
    <p class="help">No duplicates at this tolerance.</p>
  {/if}
  <div class="groups">
    {#each resolved as group, gi (gi)}
      <section class="group" class:current={gi === groupIndex} data-group={gi}>
        <div class="title">
          <span class="pill {group.kind}">{group.kind === "exact" ? "EXACT COPY" : "VERY SIMILAR"}</span>
          {group.photos.length} photos
        </div>
        <div class="row">
          {#each group.photos as photo, pi (photo.id)}
            <button
              class="tile"
              class:selected={gi === groupIndex && pi === photoIndex}
              class:deleted={photo.decision.mark === "delete"}
              data-photo={pi}
              onclick={() => {
                groupIndex = gi;
                photoIndex = pi;
              }}
              ondblclick={() => onopen(photo.id)}
            >
              <img src={thumbUrl(photo)} alt={photo.name} loading="lazy" />
              <span class="caption">{photo.folder ? `${photo.folder}/` : ""}{photo.name}</span>
              {#if photo.decision.mark}
                <span class="mark {photo.decision.mark}">{photo.decision.mark}</span>
              {/if}
            </button>
          {/each}
        </div>
      </section>
    {/each}
  </div>
</div>

<style>
  .view {
    position: fixed;
    inset: 0;
    background: var(--bg);
    z-index: 40;
    display: flex;
    flex-direction: column;
    padding: 14px 16px;
    gap: 10px;
  }
  header {
    display: flex;
    gap: 14px;
    align-items: center;
    flex-wrap: wrap;
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  .grow {
    flex: 1;
  }
  .help {
    color: var(--mut);
    margin: 0;
  }
  .error {
    color: var(--del);
  }
  .groups {
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .group {
    border: 1px solid var(--line);
    border-radius: 9px;
    padding: 10px;
    background: var(--panel);
  }
  .group.current {
    border-color: var(--acc);
  }
  .title {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 8px;
    color: var(--mut);
  }
  .pill {
    font: 600 10.5px var(--f-mono);
    padding: 2px 7px;
    border-radius: 999px;
  }
  .pill.exact {
    background: #fde3e1;
    color: var(--del);
  }
  .pill.similar {
    background: #e3eafc;
    color: var(--acc);
  }
  .row {
    display: flex;
    gap: 8px;
    overflow-x: auto;
  }
  .tile {
    position: relative;
    width: 190px;
    height: 127px;
    flex-shrink: 0;
    padding: 0;
    overflow: hidden;
    border: 2px solid transparent;
    background: var(--line);
  }
  .tile.selected {
    border-color: var(--acc);
  }
  .tile.deleted img {
    opacity: 0.35;
  }
  .tile img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .caption {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    font: 10px var(--f-mono);
    padding: 2px 4px;
    background: #000a;
    color: #fff;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mark {
    position: absolute;
    left: 6px;
    top: 6px;
    font: 600 10px var(--f-body);
    padding: 1px 6px;
    border-radius: 999px;
    color: #fff;
  }
  .mark.delete {
    background: var(--del);
  }
  .mark.unsure {
    background: var(--uns);
  }
  .mark.approved {
    background: var(--ok);
  }
</style>
