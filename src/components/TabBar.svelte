<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { displayName } from "../lib/util";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./Icon.svelte";

  const icons: Record<string, IconName> = { note: "note", pdf: "pdf", image: "image", other: "file", folder: "folder" };
</script>

<div class="tabs" data-tauri-drag-region>
  {#each app.tabs as tab (tab.id)}
    <div
      class="tab"
      class:active={tab.id === app.activeId}
      role="tab"
      tabindex="0"
      aria-selected={tab.id === app.activeId}
      title={tab.path}
      onclick={() => {
        app.activeId = tab.id;
        app.selected = tab.path;
      }}
      onauxclick={(e) => e.button === 1 && app.closeTab(tab.id)}
      onkeydown={(e) => e.key === "Enter" && (app.activeId = tab.id)}
    >
      <span class="kind {tab.kind}"><Icon name={icons[tab.kind]} size={14} /></span>
      <span class="title">{displayName(tab.path)}</span>
      <button
        class="close"
        aria-label="Close tab"
        onclick={(e) => {
          e.stopPropagation();
          app.closeTab(tab.id);
        }}
      >
        <Icon name="xmark" size={11} stroke={2.2} />
      </button>
    </div>
  {/each}
  <button class="new" title="New note" onclick={() => app.newNote()}>
    <Icon name="plus" size={15} />
  </button>
</div>

<style>
  .tabs {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 4px;
    overflow-x: auto;
    scrollbar-width: none;
    padding: 4px 2px;
  }
  .tabs::-webkit-scrollbar {
    display: none;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    max-width: 200px;
    height: 30px;
    padding: 0 6px 0 10px;
    border-radius: var(--r-pill);
    color: var(--label-2);
    font-size: 12.5px;
    font-weight: 500;
    flex: 0 1 auto;
    transition: background var(--dur) var(--ease), color var(--dur) var(--ease), box-shadow var(--dur);
  }
  .tab:hover {
    background: var(--fill-2);
    color: var(--label);
  }
  .tab.active {
    background: var(--elevated);
    color: var(--label);
    box-shadow: 0 0 0 0.5px var(--separator), 0 1px 3px rgba(0, 0, 0, 0.08), 0 4px 12px rgba(0, 0, 0, 0.05);
  }
  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .kind {
    color: var(--label-3);
  }
  .kind.pdf {
    color: var(--red);
  }
  .kind.image {
    color: var(--teal);
  }
  .tab.active .kind.note {
    color: var(--accent);
  }
  .close {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--label-3);
    opacity: 0;
    flex: none;
    transition: opacity var(--dur), background var(--dur);
  }
  .tab:hover .close,
  .tab.active .close {
    opacity: 1;
  }
  .close:hover {
    background: var(--fill);
    color: var(--label);
  }
  .new {
    display: grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--label-3);
  }
  .new:hover {
    background: var(--fill-2);
    color: var(--label);
  }
</style>
