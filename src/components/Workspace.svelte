<script lang="ts">
  import { app } from "../lib/state.svelte";
  import Sidebar from "./Sidebar.svelte";
  import Toolbar from "./Toolbar.svelte";
  import FilePane from "./FilePane.svelte";
  import RightPanel from "./RightPanel.svelte";
  import Icon from "./Icon.svelte";
  import { modKey } from "../lib/util";

  let showRight = $derived(app.rightOpen && app.activeTab?.kind === "note");
</script>

<div class="workspace" class:sidebar-open={app.sidebarOpen} class:right-open={showRight}>
  <Sidebar />

  <main class="main">
    <Toolbar />
    <div class="content">
      {#each app.tabs as tab (tab.id)}
        <div class="pane" class:active={tab.id === app.activeId}>
          {#key tab.path}
            <FilePane {tab} active={tab.id === app.activeId} />
          {/key}
        </div>
      {/each}

      {#if app.tabs.length === 0}
        <div class="empty">
          <Icon name="note" size={44} stroke={1.2} />
          <h2>No note open</h2>
          <div class="actions">
            <button class="btn primary" onclick={() => app.newNote()}>
              <Icon name="compose" size={15} /> New note
            </button>
            <button class="btn" onclick={() => (app.modal = "switcher")}>
              Open… <span class="kbd">{modKey} O</span>
            </button>
          </div>
        </div>
      {/if}
    </div>
  </main>

  {#if showRight}
    <RightPanel />
  {/if}
</div>

<style>
  .workspace {
    position: relative;
    height: 100%;
    background: var(--surface);
  }
  .main {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    transition: left 320ms var(--ease-out), right 320ms var(--ease-out);
    left: 0;
    right: 0;
  }
  .sidebar-open .main {
    left: calc(var(--sidebar-w) + var(--gutter));
  }
  .right-open .main {
    right: calc(280px + var(--gutter));
  }
  .content {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .pane {
    position: absolute;
    inset: 0;
    visibility: hidden;
  }
  .pane.active {
    visibility: visible;
  }
  .empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--label-3);
  }
  .empty h2 {
    margin: 4px 0 8px;
    color: var(--label-2);
    font: 600 17px var(--font-display);
  }
  .actions {
    display: flex;
    gap: 8px;
  }
</style>
