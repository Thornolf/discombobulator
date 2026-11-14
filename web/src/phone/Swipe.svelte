<script lang="ts">
  import ApplyDialog from "../components/ApplyDialog.svelte";
  import FolderPicker from "../components/FolderPicker.svelte";
  import { previewUrl, thumbUrl } from "../lib/api";
  import { sortPhotos } from "../lib/grouping";
  import { hamming } from "../lib/hash";
  import type { LibraryStore } from "../lib/library.svelte";
  import { cachedCount, clearOffline, downloadForOffline } from "../lib/offline";
  import { warmUp } from "../lib/preload";
  import type { Decision, Photo } from "../lib/types";
  import Piles from "./Piles.svelte";
  import { swipeDecision, type Direction } from "./swipe";

  let { store }: { store: LibraryStore } = $props();

  type Mode = "inbox" | "unsure";

  let mode = $state<Mode>("inbox");
  let deck = $state<string[]>([]);
  let position = $state(0);
  let history: number[] = [];
  let started = $state(false);
  let dragX = $state(0);
  let dragY = $state(0);
  let dragging = $state(false);
  let flying = $state<Direction | null>(null);
  let zoom = $state(false);
  let sheet = $state<"menu" | "piles" | "open" | "apply" | null>(null);
  let offline = $state<{ done: number; total: number } | null>(null);
  let offlineCount = $state<number | null>(null);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let start: { x: number; y: number } | null = null;
  let advanceTimer: ReturnType<typeof setTimeout> | undefined;

  const photos = $derived(sortPhotos(store.photos));
  const current: Photo | undefined = $derived(store.photo(deck[position] ?? ""));
  const next: Photo | undefined = $derived(store.photo(deck[position + 1] ?? ""));
  const previous: Photo | undefined = $derived(store.photo(deck[position - 1] ?? ""));
  const similar = $derived.by(() => {
    const distance = current && previous ? hamming(current.dhash, previous.dhash) : null;
    return distance !== null && distance <= 10;
  });
  const counts = $derived({
    approved: store.photos.filter((photo) => photo.decision.mark === "approved").length,
    favorite: store.photos.filter((photo) => photo.decision.favorite).length,
    unsure: store.photos.filter((photo) => photo.decision.mark === "unsure").length,
    delete: store.photos.filter((photo) => photo.decision.mark === "delete").length,
  });
  const left = $derived(Math.max(0, deck.length - position));
  const stamp = $derived.by((): Direction | null => {
    const ax = Math.abs(dragX);
    const ay = Math.abs(dragY);
    if (Math.max(ax, ay) < 30) return null;
    if (ax > ay) return dragX < 0 ? "left" : "right";
    return dragY < 0 ? "up" : "down";
  });

  function notify(message: string) {
    toast = message;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 1_600);
  }

  function begin(next: Mode) {
    mode = next;
    deck = photos
      .filter((photo) => (next === "inbox" ? !photo.decision.mark : photo.decision.mark === "unsure"))
      .map((photo) => photo.id);
    position = 0;
    history = [];
    started = true;
  }

  $effect(() => {
    if (!started && store.photos.length) begin("inbox");
  });

  $effect(() => {
    for (const id of deck.slice(position + 1, position + 5)) {
      const photo = store.photo(id);
      if (photo) warmUp(previewUrl(photo));
    }
  });

  function act(direction: Direction) {
    const photo = current;
    if (!photo || flying) return;
    const decided: Decision = swipeDecision(photo.decision, direction);
    store.decide([{ id: photo.id, decision: decided }]);
    history.push(position);
    flying = direction;
    zoom = false;
    advanceTimer = setTimeout(finishAdvance, 170);
  }

  function finishAdvance() {
    if (!flying) return;
    clearTimeout(advanceTimer);
    flying = null;
    dragX = 0;
    dragY = 0;
    position += 1;
  }

  function undo() {
    finishAdvance();
    const restored = store.undo();
    if (!restored) {
      notify("Nothing to undo");
      return;
    }
    const back = history.pop();
    if (back !== undefined) position = back;
    notify("Undone");
  }

  function pointerdown(event: PointerEvent) {
    if (zoom) return;
    start = { x: event.clientX, y: event.clientY };
    dragging = true;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function pointermove(event: PointerEvent) {
    if (!start) return;
    dragX = event.clientX - start.x;
    dragY = event.clientY - start.y;
  }

  function resetDrag() {
    start = null;
    dragging = false;
    dragX = 0;
    dragY = 0;
  }

  function pointerup() {
    if (!start) return;
    start = null;
    dragging = false;
    const ax = Math.abs(dragX);
    const ay = Math.abs(dragY);
    if (ax > 100 && ax > ay) act(dragX < 0 ? "left" : "right");
    else if (ay > 100 && ay > ax) act(dragY < 0 ? "up" : "down");
    else {
      dragX = 0;
      dragY = 0;
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (sheet) return;
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") {
      event.preventDefault();
      undo();
      return;
    }
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
    const map: Record<string, Direction> = {
      ArrowLeft: "left",
      x: "left",
      Delete: "left",
      ArrowRight: "right",
      a: "right",
      l: "right",
      ArrowUp: "up",
      f: "up",
      ArrowDown: "down",
      u: "down",
    };
    const direction = map[key];
    if (direction) {
      event.preventDefault();
      act(direction);
    } else if (key === " ") {
      event.preventDefault();
      zoom = !zoom;
    } else if (key === "p") {
      sheet = "piles";
    }
  }

  async function saveOffline() {
    offline = { done: 0, total: store.photos.length * 2 };
    try {
      const failed = await downloadForOffline(store.photos, (done, total) => (offline = { done, total }));
      notify(failed ? `${failed} files could not be saved` : "Ready to cull offline");
      offlineCount = await cachedCount(store.photos);
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error));
    } finally {
      offline = null;
    }
  }

  async function removeOffline() {
    try {
      await clearOffline();
      notify("Offline copy removed");
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error));
    }
    offlineCount = await cachedCount(store.photos);
  }

  $effect(() => {
    if (sheet === "menu") void cachedCount(store.photos).then((count) => (offlineCount = count));
  });

  const transform = $derived(
    flying
      ? ""
      : dragging || dragX || dragY
        ? `translate(${dragX}px, ${dragY}px) rotate(${dragX / 22}deg)`
        : "",
  );
</script>

<svelte:window {onkeydown} />

<div class="phone">
  <header>
    <button class="icon" onclick={() => (sheet = "menu")} aria-label="Menu">☰</button>
    <span class="grow"></span>
    <span class="left"><b>{left}</b> {mode === "inbox" ? "left" : "unsure to re-check"}</span>
    <span class="grow"></span>
    {#if !store.online}
      <span class="offline" title="Marks sync when the PC is reachable">offline · {store.pendingCount}</span>
    {/if}
    <button onclick={() => (sheet = "piles")}>Piles</button>
  </header>
  <div class="bar" aria-hidden="true">
    <i
      style:width={`${(counts.approved / Math.max(1, store.photos.length)) * 100}%`}
      style:background="var(--ok)"
    ></i>
    <i
      style:width={`${(counts.unsure / Math.max(1, store.photos.length)) * 100}%`}
      style:background="var(--uns)"
    ></i>
    <i
      style:width={`${(counts.delete / Math.max(1, store.photos.length)) * 100}%`}
      style:background="var(--del)"
    ></i>
  </div>

  <div class="deck">
    {#if current}
      {#if next}
        {#key next.id}
          <div class="card next">
            <img src={thumbUrl(next)} alt="" />
          </div>
        {/key}
      {/if}
      {#key current.id}
        <div
          class="card top"
          class:dragging
          class:zoom
          class:fly-left={flying === "left"}
          class:fly-right={flying === "right"}
          class:fly-up={flying === "up"}
          class:fly-down={flying === "down"}
          style:transform
          role="img"
          aria-label={current.name}
          onpointerdown={pointerdown}
          onpointermove={pointermove}
          onpointerup={pointerup}
          onpointercancel={resetDrag}
          ondblclick={() => (zoom = !zoom)}
        >
          <img class="thumb" src={thumbUrl(current)} alt="" />
          <img class="full" src={previewUrl(current)} alt="" />
          <span class="stamp s-left" class:show={stamp === "left"}>DELETE</span>
          <span class="stamp s-right" class:show={stamp === "right"}>LR ✓</span>
          <span class="stamp s-up" class:show={stamp === "up"}>♥ FAVORITE</span>
          <span class="stamp s-down" class:show={stamp === "down"}>UNSURE</span>
          <div class="info">
            <span class="name">{current.name}</span>
            {#if current.raf}<span class="chip">RAF</span>{/if}
            {#if similar}<span class="chip similar">Looks like the previous one</span>{/if}
            {#if mode === "unsure"}<span class="chip">Second look</span>{/if}
          </div>
        </div>
      {/key}
    {:else}
      <div class="done">
        {#if !store.photos.length}
          <h2>{store.server?.root ? "Loading…" : "No folder open"}</h2>
          <button onclick={() => (sheet = "open")}>Open a folder</button>
        {:else}
          <h2>{mode === "inbox" ? "Inbox zero" : "Second look done"}</h2>
          <div class="sum">
            <span><b style:color="var(--ok)">{counts.approved}</b>approved</span>
            <span><b style:color="var(--fav)">{counts.favorite}</b>favorites</span>
            <span><b style:color="var(--uns)">{counts.unsure}</b>unsure</span>
            <span><b style:color="var(--del)">{counts.delete}</b>delete</span>
          </div>
          <div class="actions">
            {#if counts.unsure}
              <button onclick={() => begin("unsure")}>Re-check {counts.unsure} unsure</button>
            {/if}
            <button onclick={() => (sheet = "piles")}>Review piles</button>
            <button class="primary" onclick={() => (sheet = "apply")} disabled={!store.online}>Apply</button>
          </div>
          {#if photos.some((photo) => !photo.decision.mark)}
            <button class="link" onclick={() => begin("inbox")}>Back to the inbox</button>
          {/if}
        {/if}
      </div>
    {/if}
  </div>

  <div class="buttons">
    <button class="round small" onclick={undo} aria-label="Undo">↶</button>
    <button class="round big del" onclick={() => act("left")} aria-label="Delete">✕</button>
    <button class="round uns" onclick={() => act("down")} aria-label="Unsure">?</button>
    <button class="round fav" onclick={() => act("up")} aria-label="Favorite and approve">♥</button>
    <button class="round big ok" onclick={() => act("right")} aria-label="LR approved">✓</button>
  </div>
</div>

{#if toast}
  <div class="toast" role="status">{toast}</div>
{/if}

{#if sheet === "piles"}
  <Piles
    {store}
    onback={(id) => {
      if (!deck.slice(position).includes(id))
        deck = [...deck.slice(0, position), id, ...deck.slice(position)];
    }}
    onclose={() => (sheet = null)}
  />
{:else if sheet === "menu"}
  <div class="menu" role="dialog" aria-label="Menu">
    <header>
      <b>Discombobulator</b>
      <span class="grow"></span>
      <button onclick={() => (sheet = null)}>Done</button>
    </header>
    <p class="folder">{store.server?.root ?? "No folder open"}</p>
    <button onclick={() => (sheet = "open")} disabled={!store.online}>Open another folder</button>
    <button onclick={saveOffline} disabled={!!offline || !store.photos.length || !store.online}>
      {offline ? `Saving ${offline.done}/${offline.total}…` : "Download this folder for offline"}
    </button>
    {#if offlineCount !== null}
      <p class="note">{offlineCount} of {store.photos.length} previews saved on this phone.</p>
    {/if}
    {#if offlineCount}
      <button onclick={removeOffline} disabled={!!offline}>Remove offline copy</button>
    {/if}
    <p class="note">
      {store.pendingCount
        ? `${store.pendingCount} marks waiting to sync.`
        : "All marks are synced with the PC."}
    </p>
    <button onclick={() => (sheet = "apply")} disabled={!store.online}>Apply</button>
    <a href="/?ui=desktop">Desktop view</a>
  </div>
{:else if sheet === "open"}
  <FolderPicker
    current={store.server?.root ?? null}
    onopen={async (path) => {
      await store.open(path);
      started = false;
    }}
    onclose={() => (sheet = null)}
  />
{:else if sheet === "apply"}
  <ApplyDialog
    pending={store.pendingCount}
    onapplied={async () => {
      await store.refresh();
    }}
    onclose={() => (sheet = null)}
  />
{/if}

<style>
  .phone {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: #000;
    color: #f2f2f3;
    padding-top: env(safe-area-inset-top);
    padding-bottom: env(safe-area-inset-bottom);
    max-width: 980px;
    margin: 0 auto;
    --line: #26282c;
    --panel: #121315;
    --tx: #f2f2f3;
    --bg: #000;
    color-scheme: dark;
    user-select: none;
    -webkit-user-select: none;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
  }
  header button {
    border-radius: 999px;
  }
  .icon {
    font-size: 16px;
  }
  .grow {
    flex: 1;
  }
  .left {
    font: 500 13px var(--f-mono);
    color: #8b8e95;
  }
  .left b {
    color: #f2f2f3;
    font-size: 16px;
  }
  .offline {
    font: 11px var(--f-mono);
    color: var(--uns);
  }
  .bar {
    height: 3px;
    margin: 0 14px;
    background: #1c1d20;
    border-radius: 2px;
    overflow: hidden;
    display: flex;
  }
  .bar i {
    display: block;
    height: 100%;
  }
  .deck {
    flex: 1;
    position: relative;
    margin: 12px 14px 6px;
    min-height: 0;
  }
  .card {
    position: absolute;
    inset: 0;
    border-radius: 18px;
    overflow: hidden;
    background: #0d0e10;
    display: flex;
    align-items: center;
    justify-content: center;
    touch-action: none;
  }
  .card img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
  }
  .card.next {
    transform: scale(0.94) translateY(10px);
    opacity: 0.55;
  }
  .card.top {
    box-shadow: 0 10px 40px #000;
    transition: transform 0.18s;
  }
  .card.top.dragging {
    transition: none;
  }
  .card.zoom .full {
    transform: scale(2.6);
  }
  .fly-left {
    transform: translate(-130%, 40px) rotate(-18deg);
    opacity: 0;
  }
  .fly-right {
    transform: translate(130%, 40px) rotate(18deg);
    opacity: 0;
  }
  .fly-up {
    transform: translateY(-130%);
    opacity: 0;
  }
  .fly-down {
    transform: translateY(130%);
    opacity: 0;
  }
  .stamp {
    position: absolute;
    top: 24px;
    padding: 6px 14px;
    border: 3px solid;
    border-radius: 10px;
    font-weight: 700;
    font-size: 24px;
    letter-spacing: 0.06em;
    opacity: 0;
    background: #0007;
  }
  .stamp.show {
    opacity: 1;
  }
  .s-left {
    left: 22px;
    color: var(--del);
    transform: rotate(-12deg);
  }
  .s-right {
    right: 22px;
    color: var(--ok);
    transform: rotate(12deg);
  }
  .s-up {
    top: auto;
    bottom: 90px;
    left: 50%;
    transform: translateX(-50%);
    color: var(--fav);
  }
  .s-down {
    left: 50%;
    transform: translateX(-50%);
    color: var(--uns);
  }
  .info {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    padding: 26px 16px 12px;
    background: linear-gradient(transparent, #000d);
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    align-items: flex-end;
  }
  .name {
    font: 500 13px var(--f-mono);
  }
  .chip {
    font: 600 11px var(--f-body);
    padding: 3px 9px;
    border-radius: 999px;
    background: #ffffff1f;
  }
  .chip.similar {
    background: #ffb62733;
    color: var(--uns);
  }
  .done {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    text-align: center;
  }
  .done h2 {
    margin: 0;
    font-size: 24px;
  }
  .sum {
    display: grid;
    grid-template-columns: repeat(4, auto);
    gap: 8px 18px;
    font: 500 13px var(--f-mono);
  }
  .sum b {
    display: block;
    font-size: 26px;
  }
  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: center;
  }
  .actions button {
    border-radius: 999px;
  }
  .primary {
    background: #fff;
    color: #000;
  }
  .link {
    background: none;
    border: 0;
    color: #8b8e95;
    text-decoration: underline;
  }
  .buttons {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 14px;
    padding: 10px 14px 14px;
  }
  .round {
    width: 60px;
    height: 60px;
    border-radius: 50%;
    font-size: 22px;
    font-weight: 700;
    padding: 0;
    background: #111;
    border-width: 2px;
  }
  .round.big {
    width: 70px;
    height: 70px;
    font-size: 26px;
  }
  .round.small {
    width: 44px;
    height: 44px;
    font-size: 16px;
    color: #8b8e95;
  }
  .round:active {
    transform: scale(0.92);
  }
  .del {
    border-color: #ff4f4f66;
    color: var(--del);
  }
  .uns {
    border-color: #ffb62766;
    color: var(--uns);
  }
  .fav {
    border-color: #ffd84d66;
    color: var(--fav);
  }
  .ok {
    border-color: #2fd27a66;
    color: var(--ok);
  }
  .toast {
    position: fixed;
    left: 50%;
    top: 70px;
    transform: translateX(-50%);
    background: #1d1e21;
    border: 1px solid #333;
    color: #fff;
    padding: 7px 14px;
    border-radius: 999px;
    z-index: 60;
  }
  .menu {
    position: fixed;
    inset: 0;
    background: #000;
    color: #f2f2f3;
    z-index: 45;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: calc(env(safe-area-inset-top) + 10px) 16px 16px;
    color-scheme: dark;
  }
  .menu header {
    padding: 0;
  }
  .menu button {
    padding: 12px;
    border-radius: 12px;
    background: #121315;
    border-color: #26282c;
    text-align: left;
  }
  .menu a {
    color: #8b8e95;
  }
  .folder {
    font: 12px var(--f-mono);
    color: #8b8e95;
    word-break: break-all;
  }
  .note {
    color: #8b8e95;
    margin: 0;
    font-size: 12px;
  }
</style>
