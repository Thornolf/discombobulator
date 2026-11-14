<script lang="ts">
  import {
    actionLabels,
    bindingFor,
    defaultKeymap,
    describe,
    rebind,
    type Action,
    type Keymap,
  } from "../lib/keymap";
  import Modal from "./Modal.svelte";

  let {
    keymap,
    onchange,
    onclose,
  }: { keymap: Keymap; onchange: (keymap: Keymap) => void; onclose: () => void } = $props();

  let listening = $state<Action | null>(null);

  function capture(event: KeyboardEvent) {
    if (!listening) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      listening = null;
      return;
    }
    if (["Control", "Shift", "Alt", "Meta", "AltGraph"].includes(event.key) || event.altKey) return;
    onchange(rebind(keymap, listening, bindingFor(event)));
    listening = null;
  }
</script>

<svelte:window onkeydowncapture={capture} />

<Modal title="Keyboard" {onclose}>
  <p class="intro">
    Scenes split after 5 minutes without a shot. Stacks group bursts (2.5 s apart or near-identical). Letters
    follow what is printed on your keys; numbers follow the physical number row, so AZERTY needs no Shift.
    Click a shortcut to change it.
  </p>
  <div class="keys">
    {#each Object.keys(actionLabels) as Action[] as action (action)}
      <span>{actionLabels[action]}</span>
      <button class="bind" class:listening={listening === action} onclick={() => (listening = action)}>
        {#if listening === action}
          Press a key…
        {:else}
          {#each keymap[action] as binding, index (index)}
            <kbd>{describe(binding)}</kbd>
          {:else}
            <span class="none">none</span>
          {/each}
        {/if}
      </button>
    {/each}
  </div>
  <footer>
    <button onclick={() => onchange({ ...defaultKeymap })}>Reset to defaults</button>
    <button class="primary" onclick={onclose}>Done</button>
  </footer>
</Modal>

<style>
  .intro {
    color: var(--mut);
    margin-top: 0;
  }
  .keys {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 4px 14px;
    align-items: center;
  }
  .bind {
    display: flex;
    gap: 4px;
    justify-content: flex-end;
    border-color: transparent;
    background: none;
  }
  .bind.listening {
    border-color: var(--acc);
    color: var(--acc);
  }
  .none {
    color: var(--mut);
  }
  footer {
    display: flex;
    justify-content: space-between;
    margin-top: 14px;
  }
</style>
