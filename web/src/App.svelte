<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import Stacks from "./desktop/Stacks.svelte";
  import { LibraryStore } from "./lib/library.svelte";
  import { pickShell } from "./lib/shell";
  import Swipe from "./phone/Swipe.svelte";

  const store = new LibraryStore();
  const shell = pickShell(
    window.location.search,
    window.matchMedia("(pointer: coarse)").matches,
    window.innerWidth,
  );

  onMount(() => store.start());
  onDestroy(() => store.stop());
</script>

{#if shell === "phone"}
  <Swipe {store} />
{:else}
  <Stacks {store} />
{/if}
