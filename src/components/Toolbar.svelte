<script lang="ts">
  import { copyText } from "../lib/clipboard";
  import { showMenu } from "../lib/menu.svelte";
  import { app } from "../lib/state.svelte";
  import { modKey } from "../lib/util";
  import Icon from "./Icon.svelte";
  import TabBar from "./TabBar.svelte";

  let tab = $derived(app.activeTab);

  function moreMenu(e: MouseEvent) {
    const t = app.activeTab;
    if (!t) return;
    const path = t.path;
    showMenu(e, [
      ...(app.vault?.hasHistory ? [{ label: "Version History…", icon: "clock" as const, action: () => app.showHistory(path) }, "separator" as const] : []),
      { label: "Reveal in File Manager", icon: "reveal", action: () => void app.reveal(path) },
      { label: "Copy Path", icon: "link", action: () => void copyText(path) },
      "separator",
      { label: "Delete…", icon: "trash", destructive: true, action: () => void app.remove(path) },
    ]);
  }
</script>

<header class="toolbar" data-tauri-drag-region>
  <div class="group lead">
    {#if !app.sidebarOpen}
      <button class="icon-btn" title="Show sidebar ({modKey} \)" onclick={() => (app.sidebarOpen = true)}>
        <Icon name="sidebar" size={18} />
      </button>
    {/if}
    <div class="capsule glass">
      <button class="icon-btn" title="Back" disabled={!app.canGoBack} onclick={() => app.back()}>
        <Icon name="chevron-left" size={17} />
      </button>
      <button class="icon-btn" title="Forward" disabled={!app.canGoForward} onclick={() => app.forward()}>
        <Icon name="chevron-right" size={17} />
      </button>
    </div>
  </div>

  <TabBar />

  <div class="group trail">
    {#if tab?.kind === "note"}
      <div class="segmented mode" title="Toggle reading view ({modKey} E)">
        <button class:on={tab.mode === "edit"} onclick={() => app.setMode("edit")} aria-label="Edit">
          <Icon name="pencil" size={14} />
        </button>
        <button class:on={tab.mode === "read"} onclick={() => app.setMode("read")} aria-label="Read">
          <Icon name="book" size={14} />
        </button>
      </div>
      <button
        class="icon-btn"
        class:active={app.rightOpen}
        title="Outline & backlinks"
        onclick={() => {
          app.rightOpen = !app.rightOpen;
          app.saveWorkspace();
        }}
      >
        <Icon name="sidebar-right" size={18} />
      </button>
    {:else if tab?.kind === "pdf"}
      <button
        class="icon-btn"
        class:active={app.pdfOutlineOpen}
        title="Table of contents"
        onclick={() => {
          app.pdfOutlineOpen = !app.pdfOutlineOpen;
          app.saveWorkspace();
        }}
      >
        <Icon name="sidebar-right" size={18} />
      </button>
    {/if}
    {#if tab}
      <button class="icon-btn" title="More" onclick={moreMenu}>
        <Icon name="ellipsis" size={18} />
      </button>
    {/if}
  </div>
</header>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    height: var(--toolbar-h);
    padding: 0 12px;
    flex: none;
  }
  :global(.mac) .toolbar {
    padding-left: 12px;
  }
  :global(.mac) :global(.workspace:not(.sidebar-open)) .toolbar {
    padding-left: 84px;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }
  .capsule {
    display: flex;
    padding: 2px;
    border-radius: var(--r-pill);
  }
  .capsule .icon-btn {
    width: 28px;
    height: 28px;
    border-radius: var(--r-pill);
  }
  .icon-btn:disabled {
    opacity: 0.3;
    pointer-events: none;
  }
  .mode button {
    height: 24px;
    padding: 0 10px;
  }
</style>
