<script lang="ts">
  import type { Photo } from "../lib/types";
  import { percent, tally } from "./progress";

  let { photos }: { photos: Photo[] } = $props();
  const counts = $derived(tally(photos));
</script>

<div class="bar" aria-hidden="true">
  <i style:width={percent(counts.approved, counts.total)} style:background="var(--ok)"></i>
  <i style:width={percent(counts.unsure, counts.total)} style:background="var(--uns)"></i>
  <i style:width={percent(counts.delete, counts.total)} style:background="var(--del)"></i>
</div>

<style>
  .bar {
    height: 5px;
    border-radius: 3px;
    background: var(--line);
    display: flex;
    overflow: hidden;
  }
  i {
    display: block;
    height: 100%;
  }
</style>
