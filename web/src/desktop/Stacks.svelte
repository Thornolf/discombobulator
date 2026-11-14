<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import FolderPicker from "../components/FolderPicker.svelte";
  import Help from "../components/Help.svelte";
  import { previewUrl } from "../lib/api";
  import { keepPlan, withFavorite, withMark, withRating, type MarkAction } from "../lib/decisions";
  import { groupScenes, type Scene, type Stack } from "../lib/grouping";
  import { actionFor, loadKeymap, saveKeymap, type Action, type Keymap } from "../lib/keymap";
  import type { LibraryStore } from "../lib/library.svelte";
  import { warmUp } from "../lib/preload";
  import type { Photo } from "../lib/types";
  import Frame from "./Frame.svelte";
  import SceneList from "./SceneList.svelte";
  import Strip from "./Strip.svelte";
  import { clock } from "./progress";

  let { store }: { store: LibraryStore } = $props();

  let sceneIndex = $state(0);
  let stackIndex = $state(0);
  let frameIndex = $state(0);
  let solo = $state(false);
  const selected = new SvelteSet<string>();
  let keymap = $state<Keymap>(loadKeymap());
  let dialog = $state<"open" | "help" | null>(null);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let anchorId: string | null = null;

  const scenes: Scene[] = $derived(groupScenes(store.photos));
  const scene: Scene | undefined = $derived(scenes[sceneIndex]);
  const stack: Stack = $derived(scene?.stacks[stackIndex] ?? []);
  const focused: Photo | undefined = $derived(stack[Math.min(frameIndex, stack.length - 1)]);
  const totalStacks = $derived(scenes.reduce((sum, item) => sum + item.stacks.length, 0));
  const stackNumber = $derived(
    scenes.slice(0, sceneIndex).reduce((sum, item) => sum + item.stacks.length, 0) + stackIndex + 1,
  );
  const keep = $derived(
    focused ? keepPlan(stack, selected, focused.id) : { approve: [] as string[], remove: [] as string[] },
  );
  const progress = $derived(store.server?.progress);

  function notify(message: string) {
    toast = message;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 1_800);
  }

  function locate(id: string) {
    scenes.forEach((candidate, si) =>
      candidate.stacks.forEach((frames, ki) =>
        frames.forEach((photo, fi) => {
          if (photo.id === id) {
            sceneIndex = si;
            stackIndex = ki;
            frameIndex = fi;
          }
        }),
      ),
    );
  }

  $effect(() => {
    void scenes;
    if (anchorId) locate(anchorId);
  });

  $effect(() => {
    anchorId = focused?.id ?? anchorId;
  });

  $effect(() => {
    if (sceneIndex >= scenes.length && scenes.length) sceneIndex = scenes.length - 1;
    const stacks = scenes[sceneIndex]?.stacks.length ?? 0;
    if (stackIndex >= stacks && stacks) stackIndex = stacks - 1;
  });

  $effect(() => {
    const following = neighbour(1);
    const preceding = neighbour(-1);
    for (const photo of [...(following ?? []), ...(preceding ?? [])].slice(0, 7)) {
      warmUp(previewUrl(photo));
    }
  });

  $effect(() => {
    if (store.server && !store.server.root && dialog === null) dialog = "open";
  });

  function neighbour(direction: 1 | -1): Stack | null {
    let si = sceneIndex;
    let ki = stackIndex + direction;
    while (scenes[si] && (ki < 0 || ki >= (scenes[si]?.stacks.length ?? 0))) {
      si += direction;
      const next = scenes[si];
      if (!next) return null;
      ki = direction > 0 ? 0 : next.stacks.length - 1;
    }
    return scenes[si]?.stacks[ki] ?? null;
  }

  function goTo(si: number, ki: number, fi = 0) {
    sceneIndex = si;
    stackIndex = ki;
    frameIndex = fi;
    solo = false;
    selected.clear();
  }

  function moveStack(direction: 1 | -1, landOnLast = false) {
    let si = sceneIndex;
    let ki = stackIndex + direction;
    const current = scenes[si];
    if (!current) return;
    if (ki >= current.stacks.length) {
      if (si >= scenes.length - 1) {
        notify("Last stack");
        return;
      }
      si += 1;
      ki = 0;
    }
    if (ki < 0) {
      if (si === 0) return;
      si -= 1;
      ki = (scenes[si]?.stacks.length ?? 1) - 1;
    }
    const target = scenes[si]?.stacks[ki] ?? [];
    goTo(si, ki, landOnLast ? Math.max(0, target.length - 1) : 0);
  }

  function moveFocus(direction: 1 | -1) {
    const next = frameIndex + direction;
    if (stack.length < 2 || next < 0 || next >= stack.length) {
      moveStack(direction, direction < 0);
      return;
    }
    frameIndex = next;
  }

  function moveScene(direction: 1 | -1) {
    const si = sceneIndex + direction;
    if (scenes[si]) goTo(si, 0);
  }

  function mark(action: MarkAction) {
    if (!focused) return;
    const decision = withMark(focused.decision, action);
    store.decide([{ id: focused.id, decision }]);
    if (action !== "clear" && decision.mark) moveFocus(1);
  }

  function favorite() {
    if (focused) store.decide([{ id: focused.id, decision: withFavorite(focused.decision) }]);
  }

  function rate(photo: Photo | undefined, rating: number) {
    if (photo) store.decide([{ id: photo.id, decision: withRating(photo.decision, rating) }]);
  }

  function toggleSelect() {
    if (!focused || stack.length < 2) return;
    if (selected.has(focused.id)) selected.delete(focused.id);
    else selected.add(focused.id);
  }

  function keepSelected() {
    if (!focused) return;
    const plan = keep;
    const changes = [
      ...plan.approve.flatMap((id) => {
        const photo = store.photo(id);
        return photo ? [{ id, decision: { ...photo.decision, mark: "approved" as const } }] : [];
      }),
      ...plan.remove.flatMap((id) => {
        const photo = store.photo(id);
        return photo ? [{ id, decision: { ...photo.decision, mark: "delete" as const } }] : [];
      }),
    ];
    store.decide(changes);
    notify(
      `Kept ${selected.size || 1}${plan.remove.length ? `, ${plan.remove.length} undecided → Delete` : ""}`,
    );
    moveStack(1);
  }

  function nextUndecided() {
    const flat = scenes.flatMap((item, si) => item.stacks.map((frames, ki) => ({ si, ki, frames })));
    const start = flat.findIndex((entry) => entry.si === sceneIndex && entry.ki === stackIndex);
    for (let offset = 1; offset <= flat.length; offset += 1) {
      const entry = flat[(start + offset) % flat.length];
      if (entry?.frames.some((photo) => !photo.decision.mark)) {
        goTo(entry.si, entry.ki);
        return;
      }
    }
    notify("Everything is decided 🎉");
  }

  function undo() {
    const restored = store.undo();
    if (!restored) {
      notify("Nothing to undo");
      return;
    }
    anchorId = restored[0]?.id ?? anchorId;
    if (anchorId) locate(anchorId);
    notify("Undone");
  }

  const handlers: Record<Action, () => void> = {
    approve: () => mark("approved"),
    delete: () => mark("delete"),
    unsure: () => mark("unsure"),
    favorite,
    clear: () => mark("clear"),
    undo,
    rate0: () => rate(focused, 0),
    rate1: () => rate(focused, 1),
    rate2: () => rate(focused, 2),
    rate3: () => rate(focused, 3),
    rate4: () => rate(focused, 4),
    rate5: () => rate(focused, 5),
    focusPrev: () => moveFocus(-1),
    focusNext: () => moveFocus(1),
    stackPrev: () => moveStack(-1),
    stackNext: () => moveStack(1),
    scenePrev: () => moveScene(-1),
    sceneNext: () => moveScene(1),
    solo: () => (solo = !solo),
    back: () => (solo = false),
    nextUndecided,
    apply: () => notify("Apply arrives with M3"),
    select: toggleSelect,
    keep: () => {
      if (stack.length > 1) keepSelected();
    },
    duplicates: () => notify("Duplicates arrive with M4"),
    open: () => (dialog = "open"),
    help: () => (dialog = "help"),
  };

  function onkeydown(event: KeyboardEvent) {
    if (dialog || (event.target instanceof HTMLElement && event.target.matches("input, textarea, select"))) {
      if (event.key === "Escape") dialog = null;
      return;
    }
    const action = actionFor(keymap, event);
    if (!action) return;
    event.preventDefault();
    handlers[action]();
  }

  function frameClick(index: number, event: MouseEvent) {
    frameIndex = index;
    if (event.ctrlKey || event.metaKey || event.shiftKey) toggleSelect();
  }

  function changeKeymap(next: Keymap) {
    keymap = next;
    saveKeymap(next);
  }
</script>

<svelte:window {onkeydown} />

<div class="app">
  <header>
    <span class="logo">Discombobulator</span>
    <button class="primary" onclick={() => (dialog = "open")}>Open folder</button>
    <span class="grow"></span>
    <span class="where">
      {#if scenes.length}
        Scene {sceneIndex + 1}/{scenes.length} · stack {stackNumber}/{totalStacks}
      {:else if store.server?.root}
        {store.loading ? "Loading…" : "No photos in this folder"}
      {/if}
    </span>
    <span class="grow"></span>
    {#if store.pendingCount && !store.online}
      <span class="offline" title="Marks are saved locally and sync when the app is reachable"
        >{store.pendingCount} unsynced</span
      >
    {/if}
    <button onclick={nextUndecided}>Next undecided <kbd>N</kbd></button>
    <button onclick={() => (dialog = "help")} aria-label="Keyboard shortcuts">?</button>
  </header>
  {#if progress?.running}
    <div
      class="progress"
      style:width={`${progress.total ? (progress.done / progress.total) * 100 : 0}%`}
      title={`Preparing previews: ${progress.done}/${progress.total}`}
    ></div>
  {/if}
  <main>
    <SceneList {scenes} current={sceneIndex} onpick={(index) => goTo(index, 0)} />
    <section class="center">
      {#if stack.length}
        {@const first = stack[0]}
        {@const last = stack[stack.length - 1]}
        <div class="stack-head">
          <span class="kind" class:burst={stack.length > 1}
            >{stack.length > 1 ? `BURST × ${stack.length}` : "SINGLE"}</span
          >
          {#if first && last}
            <span class="time"
              >{clock(first.time_ms)}{stack.length > 1 && last.time_ms > first.time_ms
                ? ` · ${((last.time_ms - first.time_ms) / 1000).toFixed(1)} s`
                : ""}</span
            >
          {/if}
          <span class="grow"></span>
          {#if stack.length > 1}
            <button class="keep" onclick={keepSelected}>
              Keep {selected.size ? `${selected.size} selected` : "this one"}{keep.remove.length
                ? `, delete ${keep.remove.length} undecided`
                : ""}
              <kbd>K</kbd>
            </button>
          {/if}
          <span class="hint">
            {#if stack.length > 1}
              <kbd>A</kbd> approve one · <kbd>S</kbd> select several · <kbd>K</kbd> keep only
            {:else}
              <kbd>A</kbd> approve · <kbd>X</kbd> delete · <kbd>1</kbd>–<kbd>5</kbd> stars · <kbd>↓</kbd> next
            {/if}
          </span>
        </div>
        <div class="survey">
          {#each stack as photo, index (photo.id)}
            <Frame
              {photo}
              focused={index === frameIndex && stack.length > 1}
              selected={selected.has(photo.id)}
              solo={solo && index === frameIndex}
              hidden={solo && index !== frameIndex}
              onfocus={(event) => frameClick(index, event)}
              onrate={(rating) => rate(photo, rating)}
              onsolo={() => {
                frameIndex = index;
                solo = !solo;
              }}
            />
          {/each}
        </div>
      {:else}
        <div class="empty">
          {#if store.server?.root}
            <p>{store.loading ? "Loading photos…" : "No RAF or JPG files in this folder."}</p>
          {:else}
            <p>Open a folder of RAF and JPG files to start culling.</p>
          {/if}
          <button class="primary" onclick={() => (dialog = "open")}>Open folder</button>
        </div>
      {/if}
      <Strip stacks={scene?.stacks ?? []} current={stackIndex} onpick={(index) => goTo(sceneIndex, index)} />
    </section>
  </main>
</div>

{#if toast}
  <div class="toast" role="status">{toast}</div>
{/if}

{#if dialog === "open"}
  <FolderPicker
    current={store.server?.root ?? null}
    onopen={async (path) => {
      anchorId = null;
      await store.open(path);
      goTo(0, 0);
    }}
    onclose={() => (dialog = null)}
  />
{:else if dialog === "help"}
  <Help {keymap} onchange={changeKeymap} onclose={() => (dialog = null)} />
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  header {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 9px 14px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
  }
  .logo {
    font-weight: 700;
    font-size: 15px;
  }
  .grow {
    flex: 1;
  }
  .where {
    font: 12px var(--f-mono);
    color: var(--mut);
  }
  .offline {
    font: 11px var(--f-mono);
    color: var(--uns);
  }
  .progress {
    height: 2px;
    background: var(--acc);
    transition: width 0.3s;
  }
  main {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .stack-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 14px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
  }
  .kind {
    font: 600 11px var(--f-mono);
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--bg);
  }
  .kind.burst {
    color: var(--acc);
  }
  .time {
    font: 12px var(--f-mono);
  }
  .keep {
    border-color: var(--ok);
    color: var(--ok);
  }
  .hint {
    color: var(--mut);
    font-size: 12px;
  }
  .survey {
    flex: 1;
    background: var(--stage);
    display: flex;
    gap: 10px;
    padding: 14px;
    min-height: 0;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--mut);
    background: var(--stage);
  }
  .empty p {
    color: #fff;
  }
  .toast {
    position: fixed;
    left: 50%;
    bottom: 120px;
    transform: translateX(-50%);
    background: var(--tx);
    color: var(--bg);
    padding: 7px 14px;
    border-radius: 8px;
    z-index: 60;
  }
</style>
