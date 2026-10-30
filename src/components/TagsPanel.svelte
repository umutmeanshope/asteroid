<script lang="ts">
  import { app } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";

  function openTag(tag: string) {
    app.searchQuery = `#${tag}`;
    app.sidebarView = "search";
  }
</script>

<div class="tags">
  {#each app.tags as { tag, count } (tag)}
    <button class="tag" onclick={() => openTag(tag)}>
      <Icon name="hash" size={13} />
      <span class="name">{tag}</span>
      <span class="count">{count}</span>
    </button>
  {:else}
    <p class="hint">No tags yet. Add <code>#tag</code> to a note.</p>
  {/each}
</div>

<style>
  .tags {
    flex: 1;
    overflow-y: auto;
    padding: 0 8px 12px;
  }
  .tag {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--label);
    text-align: left;
  }
  .tag:hover {
    background: var(--fill-3);
  }
  .tag :global(svg) {
    color: var(--accent);
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    min-width: 22px;
    padding: 1px 7px;
    border-radius: var(--r-pill);
    background: var(--fill-2);
    color: var(--label-2);
    font-size: 11px;
    text-align: center;
  }
  .hint {
    margin: 24px 12px;
    color: var(--label-3);
    text-align: center;
  }
  code {
    font-family: var(--font-mono);
    font-size: 12px;
  }
</style>
