<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { fileName } from "../lib/util";
  import Icon from "./Icon.svelte";

  let { path }: { path: string } = $props();
</script>

<div class="other">
  <div class="card">
    <Icon name="file" size={48} stroke={1.2} />
    <h2>{fileName(path)}</h2>
    <p>Asteroid can’t preview this kind of file.</p>
    <button class="btn" onclick={() => app.reveal(path)}>
      <Icon name="reveal" size={15} /> Reveal in File Manager
    </button>
  </div>
</div>

<style>
  .other {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
  }
  .card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    color: var(--label-3);
    text-align: center;
  }
  h2 {
    margin: 8px 0 0;
    color: var(--label);
    font: 600 17px var(--font-display);
  }
  p {
    margin: 0 0 12px;
    color: var(--label-2);
  }
</style>
