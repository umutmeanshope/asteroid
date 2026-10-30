<script lang="ts">
  import { showMenu, type MenuItem } from "../lib/menu.svelte";
  import { api, message } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import type { RecentVault } from "../lib/types";
  import { modKey, parentOf, relativeTime } from "../lib/util";
  import Icon from "./Icon.svelte";
  import FileTree from "./FileTree.svelte";
  import SearchPanel from "./SearchPanel.svelte";
  import TagsPanel from "./TagsPanel.svelte";

  // Re-render the relative "synced … ago" label periodically.
  let now = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(t);
  });

  let syncLabel = $derived.by(() => {
    void now;
    if (!app.vault?.sync) return "Not synced";
    if (app.sync.running) return "Syncing…";
    if (app.sync.error) return "Sync failed";
    if (app.sync.lastAt) return `Synced ${relativeTime(app.sync.lastAt)}`;
    return app.vault.sync.provider === "git" ? "GitHub" : "Google Drive";
  });

  async function vaultMenu(e: MouseEvent) {
    const button = e.currentTarget as HTMLElement;
    const recent = (await api.recentVaults().catch(() => [])).slice(0, 8);
    // Vaults that share a name get their parent folder appended to tell them apart.
    const label = (v: RecentVault) =>
      recent.filter((r) => r.name === v.name).length > 1 ? `${v.name} — ${parentOf(v.path) || "/"}` : v.name;
    const vaults: MenuItem[] = recent.map((v) => ({
      label: label(v),
      checked: v.path === app.vault?.path,
      action: () => {
        if (v.path !== app.vault?.path) app.openVault(v.path).catch((err) => app.toast(message(err), "error"));
      },
    }));
    showMenu(e, [
      ...vaults,
      ...(vaults.length ? ["separator" as const] : []),
      { label: "Open Another Vault…", icon: "folder", action: () => void app.pickAndOpenVault() },
      { label: "Open Vault from Google Drive…", icon: "drive", action: () => (app.modal = "drive-pull") },
      { label: "Open Vault from GitHub…", icon: "git", action: () => (app.modal = "git-pull") },
      "separator",
      { label: "Reveal Vault Folder", icon: "reveal", action: () => void app.reveal("") },
      ...(app.vault?.hasHistory ? [{ label: "Recently Deleted…", icon: "trash" as const, action: () => app.showHistory(null) }] : []),
      { label: "Close Vault", icon: "xmark", action: () => void app.closeVault() },
    ], button);
  }

  function syncClick() {
    if (app.vault?.sync) void app.syncNow();
    else {
      app.settingsSection = "sync";
      app.modal = "settings";
    }
  }
</script>

<aside class="sidebar glass" class:hidden={!app.sidebarOpen}>
  <div class="head" data-tauri-drag-region>
    <button class="vault" onclick={vaultMenu} title={app.vault?.path}>
      <span class="vault-icon"><Icon name="vault" size={14} /></span>
      <span class="vault-name">{app.vault?.name}</span>
      <Icon name="chevron-up-down" size={12} />
    </button>
    <div class="head-actions">
      <button class="icon-btn" title="Hide sidebar ({modKey} \)" onclick={() => (app.sidebarOpen = false)}>
        <Icon name="sidebar" size={17} />
      </button>
    </div>
  </div>

  <div class="tools">
    <div class="segmented views">
      <button class:on={app.sidebarView === "files"} onclick={() => (app.sidebarView = "files")} title="Files">
        <Icon name="folder" size={14} />
      </button>
      <button class:on={app.sidebarView === "search"} onclick={() => (app.sidebarView = "search")} title="Search ({modKey} ⇧ F)">
        <Icon name="search" size={14} />
      </button>
      <button class:on={app.sidebarView === "tags"} onclick={() => (app.sidebarView = "tags")} title="Tags">
        <Icon name="hash" size={14} />
      </button>
    </div>
    <button class="icon-btn" title="New note ({modKey} N)" onclick={() => app.newNote()}>
      <Icon name="compose" size={17} />
    </button>
    <button class="icon-btn" title="New folder ({modKey} ⇧ N)" onclick={() => app.newFolder()}>
      <Icon name="folder-plus" size={17} />
    </button>
  </div>

  <div class="body">
    {#if app.sidebarView === "files"}
      <FileTree />
    {:else if app.sidebarView === "search"}
      <SearchPanel />
    {:else}
      <TagsPanel />
    {/if}
  </div>

  <footer class="foot">
    <button class="sync" class:error={!!app.sync.error} onclick={syncClick} title={app.sync.error ?? "Sync now"}>
      {#if app.sync.running}
        <span class="spinner"></span>
      {:else if app.sync.error}
        <Icon name="warning" size={15} />
      {:else if app.vault?.sync}
        <Icon name="cloud-check" size={16} />
      {:else}
        <Icon name="cloud-off" size={16} />
      {/if}
      <span>{syncLabel}</span>
    </button>
    <button
      class="icon-btn"
      title="Settings ({modKey} ,)"
      onclick={() => {
        app.settingsSection = "general";
        app.modal = "settings";
      }}
    >
      <Icon name="gear" size={16} />
    </button>
  </footer>
</aside>

<style>
  .sidebar {
    position: absolute;
    z-index: 5;
    top: var(--gutter);
    bottom: var(--gutter);
    left: var(--gutter);
    width: var(--sidebar-w);
    display: flex;
    flex-direction: column;
    border-radius: 18px;
    background: var(--glass);
    transition: transform 320ms var(--ease-out), opacity 240ms var(--ease);
  }
  :global([data-appearance="light"]) .sidebar {
    background: rgba(242, 242, 247, 0.78);
  }
  .sidebar.hidden {
    transform: translateX(calc(-100% - var(--gutter) * 2));
    opacity: 0;
    pointer-events: none;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 46px;
    padding: 6px 8px 0 8px;
  }
  :global(.mac) .head {
    padding-left: 76px;
  }
  .vault {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    height: 30px;
    padding: 0 8px 0 4px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--label);
    font-weight: 600;
    font-size: 13px;
    text-align: left;
  }
  .vault:hover {
    background: var(--fill-2);
  }
  .vault-icon {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: linear-gradient(160deg, color-mix(in srgb, var(--accent) 80%, white), var(--accent));
    color: #fff;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
    flex: none;
  }
  .vault-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .vault :global(svg:last-child) {
    color: var(--label-3);
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 6px 8px 8px;
  }
  .views {
    flex: 1;
    margin-right: 4px;
  }
  .views button {
    display: grid;
    place-items: center;
    padding: 0;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 8px 8px;
    border-top: 0.5px solid var(--separator);
  }
  .sync {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    height: 30px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--label-2);
    font-size: 12px;
    text-align: left;
  }
  .sync span:last-child {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sync:hover {
    background: var(--fill-2);
    color: var(--label);
  }
  .sync.error {
    color: var(--red);
  }
</style>
