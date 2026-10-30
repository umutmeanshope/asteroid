<script lang="ts">
  import { app } from "../lib/state.svelte";
  import TreeNode from "./TreeNode.svelte";
  import { parentOf } from "../lib/util";

  let rootDrop = $state(false);

  function onDrop(e: DragEvent) {
    rootDrop = false;
    const path = e.dataTransfer?.getData("application/x-asteroid-path");
    if (path && parentOf(path) !== "") {
      e.preventDefault();
      void app.move(path, "");
    }
  }
</script>

<div
  class="tree"
  class:drop={rootDrop}
  role="tree"
  tabindex="-1"
  ondragover={(e) => {
    if (e.dataTransfer?.types.includes("application/x-asteroid-path")) {
      e.preventDefault();
      rootDrop = e.target === e.currentTarget;
    }
  }}
  ondragleave={() => (rootDrop = false)}
  ondrop={onDrop}
  onclick={(e) => {
    if (e.target === e.currentTarget) app.selected = null;
  }}
  onkeydown={() => {}}
>
  {#each app.tree as entry (entry.path)}
    <TreeNode {entry} depth={0} />
  {:else}
    <p class="hint">This vault is empty.<br />Create a note to get started.</p>
  {/each}
</div>

<style>
  .tree {
    flex: 1;
    overflow-y: auto;
    padding: 2px 8px 12px;
    outline: none;
    border-radius: 12px;
  }
  .tree.drop {
    background: var(--accent-tint);
  }
  .hint {
    margin: 24px 12px;
    color: var(--label-3);
    text-align: center;
    line-height: 1.6;
  }
</style>
