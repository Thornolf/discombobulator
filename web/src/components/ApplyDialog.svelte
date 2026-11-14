<script lang="ts">
  import { api } from "../lib/api";
  import type { ApplyPlan, ApplyReport } from "../lib/types";
  import Modal from "./Modal.svelte";
  import RecycleSection from "./RecycleSection.svelte";

  let {
    pending,
    onapplied,
    onclose,
  }: { pending: number; onapplied: () => Promise<void>; onclose: () => void } = $props();

  let plan = $state<ApplyPlan | null>(null);
  let report = $state<ApplyReport | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  async function load() {
    try {
      plan = await api.applyPlan();
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure);
    }
  }

  async function run() {
    busy = true;
    error = null;
    try {
      report = await api.apply();
      await onapplied();
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure);
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    void load();
  });
</script>

<Modal title="Apply" {onclose}>
  {#if report}
    <p>
      Wrote <b>{report.sidecars_written}</b> XMP sidecar{report.sidecars_written === 1 ? "" : "s"} and moved
      <b>{report.photos_moved}</b> photo{report.photos_moved === 1 ? "" : "s"} ({report.files_moved} files) to
      <code>_to_delete</code>.
    </p>
    {#if report.failures.length}
      <p class="error">{report.failures.length} could not be applied:</p>
      <ul class="failures">
        {#each report.failures as failure, index (index)}
          <li><b>{failure.name}</b>: {failure.error}</li>
        {/each}
      </ul>
    {/if}
    <footer><button class="primary" onclick={onclose}>Done</button></footer>
  {:else}
    {#if pending}
      <p class="warn">
        {pending} mark{pending === 1 ? " is" : "s are"} still syncing. Wait a moment before applying.
      </p>
    {/if}
    {#if plan}
      <ul class="plan">
        <li>
          Write <b>{plan.sidecars}</b> XMP sidecar{plan.sidecars === 1 ? "" : "s"} for Lightroom (picks, rejects,
          stars, labels)
        </li>
        <li>
          Move <b>{plan.deletes}</b> photo{plan.deletes === 1 ? "" : "s"} marked Delete ({plan.files_to_move} files,
          RAF + JPG together) to a <code>_to_delete</code> folder next to them
        </li>
      </ul>
      <p class="note">
        Nothing is erased. Once you have checked <code>_to_delete</code>, you can send it to the Recycle Bin
        below.
      </p>
    {:else if !error}
      <p>Counting…</p>
    {/if}
    {#if error}
      <p class="error">{error}</p>
    {/if}
    <footer>
      <button onclick={onclose}>Cancel</button>
      <button
        class="primary"
        disabled={!plan || busy || pending > 0 || (plan.sidecars === 0 && plan.deletes === 0)}
        onclick={run}
      >
        {busy ? "Applying…" : "Apply"}
      </button>
    </footer>
  {/if}
  <RecycleSection generation={report ? 1 : 0} />
</Modal>

<style>
  .plan {
    padding-left: 18px;
  }
  .note {
    color: var(--mut);
  }
  .warn {
    color: var(--uns);
  }
  .error {
    color: var(--del);
  }
  .failures {
    max-height: 200px;
    overflow: auto;
    font-size: 12px;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
</style>
