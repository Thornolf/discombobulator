<script lang="ts">
  import { api } from "../lib/api";
  import type { FolderListing } from "../lib/types";
  import Modal from "./Modal.svelte";

  let {
    current,
    onopen,
    onclose,
  }: { current: string | null; onopen: (path: string) => Promise<void>; onclose: () => void } = $props();

  let listing = $state<FolderListing | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  async function browse(path?: string) {
    error = null;
    try {
      listing = await api.folders(path);
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure);
    }
  }

  async function choose(path: string) {
    busy = true;
    error = null;
    try {
      await onopen(path);
      onclose();
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure);
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    void browse(current ?? undefined);
  });
</script>

<Modal title="Open a folder" {onclose}>
  <div class="path">
    <button onclick={() => browse()} title="Drives and shortcuts">⌂</button>
    {#if listing?.parent}
      <button onclick={() => browse(listing?.parent ?? undefined)} title="Parent folder">↑</button>
    {/if}
    <code>{listing?.path ?? "Drives and shortcuts"}</code>
  </div>
  {#if error}
    <p class="error">{error}</p>
  {/if}
  <ul>
    {#each listing?.entries ?? [] as entry (entry.path)}
      <li>
        <button class="entry" ondblclick={() => choose(entry.path)} onclick={() => browse(entry.path)}>
          📁 {entry.name}
        </button>
      </li>
    {:else}
      <li class="empty">No subfolders</li>
    {/each}
  </ul>
  <footer>
    <button onclick={onclose}>Cancel</button>
    <button
      class="primary"
      disabled={!listing?.path || busy}
      onclick={() => listing?.path && choose(listing.path)}
    >
      {busy ? "Opening…" : "Open this folder"}
    </button>
  </footer>
</Modal>

<style>
  .path {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-bottom: 8px;
  }
  code {
    font: 12px var(--f-mono);
    color: var(--mut);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 50vh;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  .entry {
    width: 100%;
    text-align: left;
    border: 0;
    border-radius: 0;
    border-bottom: 1px solid var(--line);
  }
  .empty {
    padding: 10px;
    color: var(--mut);
  }
  .error {
    color: var(--del);
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
</style>
