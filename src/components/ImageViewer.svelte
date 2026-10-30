<script lang="ts">
  import { api, message } from "../lib/api";
  import { fileName, vaultBlobUrl } from "../lib/util";
  import Icon from "./Icon.svelte";

  let { path }: { path: string } = $props();
  let src = $state<string | null>(null);
  let error = $state<string | null>(null);
  let actual = $state(false);

  $effect(() => {
    vaultBlobUrl(path, api.readFile)
      .then((url) => (src = url))
      .catch((e) => (error = message(e)));
  });
</script>

<div class="viewer" class:actual>
  {#if error}
    <div class="state"><Icon name="warning" size={30} stroke={1.4} /><p>{error}</p></div>
  {:else if src}
    <button class="frame" onclick={() => (actual = !actual)} title={actual ? "Fit to window" : "Actual size"}>
      <img {src} alt={fileName(path)} />
    </button>
  {:else}
    <div class="state"><span class="spinner"></span></div>
  {/if}
</div>

<style>
  .viewer {
    position: absolute;
    inset: 0;
    overflow: auto;
    display: grid;
    place-items: center;
    padding: 24px;
  }
  .frame {
    padding: 0;
    border: 0;
    background: none;
    cursor: zoom-in;
    max-width: 100%;
    max-height: 100%;
  }
  .actual .frame {
    cursor: zoom-out;
    max-width: none;
    max-height: none;
  }
  img {
    display: block;
    max-width: 100%;
    max-height: calc(100vh - 140px);
    border-radius: var(--r-lg);
    box-shadow: var(--shadow-lg);
    background: repeating-conic-gradient(var(--fill-3) 0% 25%, transparent 0% 50%) 50% / 20px 20px;
  }
  .actual img {
    max-width: none;
    max-height: none;
  }
  .state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    color: var(--label-3);
  }
</style>
