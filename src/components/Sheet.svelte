<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    onclose,
    width = 560,
    placement = "top",
    label,
    children,
  }: {
    onclose: () => void;
    width?: number;
    placement?: "top" | "center";
    label: string;
    children: Snippet;
  } = $props();
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    }
  }}
/>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" class:center={placement === "center"} onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sheet" role="dialog" aria-modal="true" aria-label={label} style:width="min({width}px, calc(100vw - 32px))">
    {@render children()}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
    background: var(--scrim);
    animation: fade-in 160ms var(--ease);
  }
  .scrim.center {
    align-items: center;
    padding-top: 0;
  }
  .sheet {
    max-height: calc(100vh - 24vh);
    display: flex;
    flex-direction: column;
    border-radius: var(--r-xl);
    background: var(--glass-strong);
    backdrop-filter: saturate(180%) blur(40px);
    -webkit-backdrop-filter: saturate(180%) blur(40px);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
    animation: sheet-in 260ms var(--ease-out);
  }
  .center .sheet {
    max-height: calc(100vh - 48px);
  }
</style>
