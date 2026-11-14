<script lang="ts">
  import { api } from "../lib/api";
  import type { PhoneStatus } from "../lib/types";
  import Modal from "./Modal.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let status = $state<PhoneStatus | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function run(action: () => Promise<PhoneStatus>) {
    busy = true;
    error = null;
    try {
      status = await action();
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure);
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    void run(api.phone);
  });

  const qr = $derived(
    status?.qr_svg ? `data:image/svg+xml;charset=utf-8,${encodeURIComponent(status.qr_svg)}` : null,
  );
</script>

<Modal title="Cull on your iPhone" {onclose}>
  {#if !status && !error}
    <p>Checking Tailscale…</p>
  {:else if status && !status.tailscale}
    <p>
      Install <b>Tailscale</b> on this PC and on your iPhone, sign in to the same account on both, then come back
      here.
    </p>
  {:else if status?.url}
    <div class="ready">
      {#if qr}
        <img src={qr} alt="QR code for the phone address" width="220" height="220" />
      {/if}
      <div>
        <p>Open this address in Safari on your iPhone:</p>
        <code>{status.url}</code>
        <ol>
          <li>Tap the Share button, then <b>Add to Home Screen</b>.</li>
          <li>Open <b>Cull</b> from the Home Screen. It runs full screen.</li>
          <li>In its menu, <b>Download this folder for offline</b> to cull without the PC.</li>
        </ol>
        <p class="note">
          Only devices on your tailnet can reach it. The phone downloads previews, never RAFs.
        </p>
      </div>
    </div>
  {:else}
    <p>
      Share Discombobulator on your tailnet so your iPhone can reach it over HTTPS. This runs
      <code>tailscale serve</code> for this app only.
    </p>
    <button class="primary" disabled={busy} onclick={() => run(api.enablePhone)}>
      {busy ? "Setting up…" : "Enable phone access"}
    </button>
  {/if}
  {#if status?.error}
    <p class="error">{status.error}</p>
  {/if}
  {#if status?.action_url}
    <p>
      <a href={status.action_url} target="_blank" rel="noopener noreferrer">{status.action_url}</a>
    </p>
  {/if}
  {#if error}
    <p class="error">{error}</p>
  {/if}
</Modal>

<style>
  .ready {
    display: flex;
    gap: 18px;
    align-items: flex-start;
    flex-wrap: wrap;
  }
  img {
    background: #fff;
    border-radius: 8px;
  }
  code {
    font: 13px var(--f-mono);
    word-break: break-all;
  }
  .note {
    color: var(--mut);
  }
  .error {
    color: var(--del);
  }
  a {
    word-break: break-all;
  }
</style>
