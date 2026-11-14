<script lang="ts">
  import { api, thumbUrl } from "../lib/api";
  import { keepPlan, withFavorite, withMark, withRating, type MarkAction } from "../lib/decisions";
  import { actionFor, describe, type Action, type Keymap } from "../lib/keymap";
  import type { LibraryStore } from "../lib/library.svelte";
  import type { DuplicateGroup, Photo } from "../lib/types";

  let {
    store,
    keymap,
    onopen,
    onclose,
  }: { store: LibraryStore; keymap: Keymap; onopen: (id: string) => void; onclose: () => void } = $props();

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

  function rate(rating: number) {
    if (current) store.decide([{ id: current.id, decision: withRating(current.decision, rating) }]);
  }

  function moveGroup(direction: 1 | -1) {
    const target = groupIndex + direction;
    if (target < 0 || target >= resolved.length) return;
    groupIndex = target;
    photoIndex = 0;
  }

  const handlers: Partial<Record<Action, () => void>> = {
    focusNext: () => (photoIndex = Math.min(photoIndex + 1, (resolved[groupIndex]?.photos.length ?? 1) - 1)),
    focusPrev: () => (photoIndex = Math.max(0, photoIndex - 1)),
    stackNext: () => moveGroup(1),
    stackPrev: () => moveGroup(-1),
    keep: keepOne,
    approve: () => mark("approved"),
    delete: () => mark("delete"),
    unsure: () => mark("unsure"),
    clear: () => mark("clear"),
    favorite: () => {
      if (current) store.decide([{ id: current.id, decision: withFavorite(current.decision) }]);
    },
    undo: () => store.undo(),
    rate0: () => rate(0),
    rate1: () => rate(1),
    rate2: () => rate(2),
    rate3: () => rate(3),
    rate4: () => rate(4),
    rate5: () => rate(5),
    solo: () => {
      if (current) onopen(current.id);
    },
    back: () => onclose(),
    duplicates: () => onclose(),
  };

  function keys(action: Action): string {
    return keymap[action].map(describe).join("/");
  }

  function onkeydown(event: KeyboardEvent) {
    const action = actionFor(keymap, event);
    if (event.target instanceof HTMLInputElement && action !== "undo" && action !== "back") return;
    const handler = action ? handlers[action] : undefined;
    if (!handler) return;
    event.preventDefault();
    event.stopPropagation();
    handler();
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
    <button onclick={onclose}>Close <kbd>{keys("back")}</kbd></button>
  </header>
  <p class="help">
    Exact = same bytes. Similar = perceptual hash within the tolerance. <kbd>{keys("stackPrev")}</kbd><kbd
      >{keys("stackNext")}</kbd
    >
    group, <kbd>{keys("focusPrev")}</kbd><kbd>{keys("focusNext")}</kbd> photo,
    <kbd>{keys("keep")}</kbd> keep this one and mark the undecided rest Delete, <kbd>{keys("solo")}</kbd> open in
    Stacks.
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
