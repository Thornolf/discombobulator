<script lang="ts">
  import { api } from "../lib/api";
  import type { RecyclePlan, RecycleReport } from "../lib/types";

  let { generation }: { generation: number } = $props();

  let plan = $state<RecyclePlan | null>(null);
  let report = $state<RecycleReport | null>(null);
  let confirming = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const megabytes = (bytes: number) => `${(bytes / 1_048_576).toFixed(bytes > 10_485_760 ? 0 : 1)} MB`;

  async function load() {
    try {
      plan = await api.recyclePlan();
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure);
    }
  }

  async function send() {
    if (!plan) return;
    busy = true;
    error = null;
    try {
      report = await api.recycle(plan);
      confirming = false;
      await load();
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure);
      confirming = false;
      await load();
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    void generation;
    void load();
  });
</script>

{#if report || (plan && plan.files > 0) || error}
  <section>
    {#if report}
      <p>
        Sent <b>{report.recycled}</b> file{report.recycled === 1 ? "" : "s"} ({megabytes(report.bytes)}) to
        the Recycle Bin. Restore them from there if needed.
      </p>
      {#if report.failures.length}
        <ul class="failures">
          {#each report.failures as failure, index (index)}
            <li><b>{failure.name}</b>: {failure.error}</li>
          {/each}
        </ul>
      {/if}
    {/if}
    {#if plan && plan.files > 0}
      {#if confirming}
        <p>
          Send <b>{plan.files}</b> file{plan.files === 1 ? "" : "s"} ({megabytes(plan.bytes)}) from
          <code>_to_delete</code> to the Recycle Bin?
        </p>
        <div class="row">
          <button onclick={() => (confirming = false)} disabled={busy}>Cancel</button>
          <button class="danger" onclick={send} disabled={busy}
            >{busy ? "Sending…" : "Send to Recycle Bin"}</button
          >
        </div>
      {:else}
        <div class="row">
          <span class="note"
            ><code>_to_delete</code> holds {plan.files} file{plan.files === 1 ? "" : "s"} ({megabytes(
              plan.bytes,
            )}).</span
          >
          <button onclick={() => (confirming = true)}>Empty into Recycle Bin…</button>
        </div>
      {/if}
    {/if}
    {#if error}
      <p class="error">{error}</p>
    {/if}
  </section>
{/if}

<style>
  section {
    border-top: 1px solid var(--line);
    margin-top: 14px;
    padding-top: 10px;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
    flex-wrap: wrap;
  }
  .note {
    color: var(--mut);
    margin-right: auto;
  }
  .danger {
    border-color: var(--del);
    color: var(--del);
  }
  .error {
    color: var(--del);
  }
  .failures {
    max-height: 160px;
    overflow: auto;
    font-size: 12px;
  }
</style>
