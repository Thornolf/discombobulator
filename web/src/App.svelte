<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import Stacks from "./desktop/Stacks.svelte";
  import { LibraryStore } from "./lib/library.svelte";

  const store = new LibraryStore();

  onMount(() => store.start());
  onDestroy(() => store.stop());
</script>

<Stacks {store} />
