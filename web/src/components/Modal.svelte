<script lang="ts">
  import type { Snippet } from "svelte";

  let { title, onclose, children }: { title: string; onclose: () => void; children: Snippet } = $props();
</script>

<div
  class="backdrop"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) onclose();
  }}
>
  <div class="card" role="dialog" aria-modal="true" aria-label={title}>
    <header>
      <h2>{title}</h2>
      <button onclick={onclose} aria-label="Close">✕</button>
    </header>
    {@render children()}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: #0008;
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
    padding: 16px;
  }
  .card {
    background: var(--panel);
    color: var(--tx);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 16px 18px;
    width: min(680px, 100%);
    max-height: 90%;
    overflow: auto;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
</style>
