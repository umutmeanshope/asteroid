<script lang="ts">
  import { api, message } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import type { DriveFolder, DriveStatus } from "../lib/types";
  import Icon from "./Icon.svelte";

  let {
    action,
    onpick,
    busy = false,
  }: {
    /** Label for the primary button, given the current folder's name. */
    action: (name: string) => string;
    onpick: (folder: DriveFolder) => void;
    busy?: boolean;
  } = $props();

  let status = $state<DriveStatus | null>(null);
  let stack = $state<DriveFolder[]>([{ id: "root", name: "My Drive" }]);
  let folders = $state<DriveFolder[]>([]);
  let loading = $state(false);
  let connecting = $state(false);
  let error = $state<string | null>(null);
  let newName = $state<string | null>(null);

  let current = $derived(stack[stack.length - 1]);

  async function refreshStatus() {
    status = await api.driveStatus();
  }

  async function load(id: string) {
    loading = true;
    error = null;
    try {
      folders = await api.driveListFolders(id);
    } catch (e) {
      error = message(e);
      folders = [];
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void refreshStatus();
  });

  $effect(() => {
    if (status?.connected) void load(current.id);
  });

  async function connect() {
    connecting = true;
    try {
      const email = await api.driveConnect();
      app.toast(`Connected as ${email}`, "success");
      await refreshStatus();
    } catch (e) {
      app.toast(message(e), "error");
    } finally {
      connecting = false;
    }
  }

  async function createFolder() {
    const name = newName?.trim();
    newName = null;
    if (!name) return;
    try {
      const folder = await api.driveCreateFolder(name, current.id);
      stack.push(folder);
    } catch (e) {
      app.toast(message(e), "error");
    }
  }
</script>

<div class="browser">
  {#if !status}
    <div class="center"><span class="spinner"></span></div>
  {:else if !status.configured}
    <div class="center">
      <Icon name="drive" size={34} stroke={1.3} />
      <p>Add your Google OAuth client ID in <strong>Settings › Sync</strong> to use Google Drive.</p>
      <button
        class="btn"
        onclick={() => {
          app.settingsSection = "sync";
          app.modal = "settings";
        }}>Open Sync Settings</button
      >
    </div>
  {:else if !status.connected}
    <div class="center">
      <Icon name="drive" size={34} stroke={1.3} />
      <p>Sign in with Google to browse your Drive folders.</p>
      <button class="btn primary" disabled={connecting} onclick={connect}>
        {#if connecting}<span class="spinner"></span> Waiting for browser…{:else}Connect Google Drive{/if}
      </button>
    </div>
  {:else}
    <nav class="crumbs">
      {#each stack as folder, i (folder.id)}
        {#if i > 0}<Icon name="chevron-right" size={11} stroke={2} />{/if}
        <button class:last={i === stack.length - 1} onclick={() => (stack = stack.slice(0, i + 1))}>{folder.name}</button>
      {/each}
    </nav>

    <div class="list">
      {#if loading}
        <div class="center small"><span class="spinner"></span></div>
      {:else if error}
        <div class="center small error">{error}</div>
      {:else}
        {#each folders as folder (folder.id)}
          <button class="folder" ondblclick={() => stack.push(folder)} onclick={() => stack.push(folder)}>
            <Icon name="folder" size={17} />
            <span>{folder.name}</span>
            <Icon name="chevron-right" size={12} stroke={2} />
          </button>
        {:else}
          <div class="center small">No folders here</div>
        {/each}
      {/if}
      {#if newName !== null}
        <div class="folder new">
          <Icon name="folder-plus" size={17} />
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="field"
            autofocus
            placeholder="Folder name"
            bind:value={newName}
            onkeydown={(e) => {
              if (e.key === "Enter") void createFolder();
              if (e.key === "Escape") {
                e.stopPropagation();
                newName = null;
              }
            }}
          />
        </div>
      {/if}
    </div>

    <div class="actions">
      <button class="btn" onclick={() => (newName = "")}><Icon name="folder-plus" size={15} /> New Folder</button>
      <span class="grow"></span>
      <button class="btn primary" disabled={busy || current.id === "root"} onclick={() => onpick(current)}>
        {#if busy}<span class="spinner"></span>{/if}
        {action(current.name)}
      </button>
    </div>
    {#if current.id === "root"}
      <p class="note">Open a folder to use it — syncing all of My Drive isn’t supported.</p>
    {/if}
  {/if}
</div>

<style>
  .browser {
    display: flex;
    flex-direction: column;
    min-height: 280px;
  }
  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 24px;
    color: var(--label-3);
    text-align: center;
  }
  .center p {
    margin: 0 0 6px;
    max-width: 320px;
    color: var(--label-2);
  }
  .center.small {
    padding: 28px;
  }
  .center.error {
    color: var(--red);
  }
  .crumbs {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 2px;
    margin-bottom: 8px;
    color: var(--label-3);
  }
  .crumbs button {
    border: 0;
    background: none;
    padding: 3px 6px;
    border-radius: 6px;
    color: var(--label-2);
    font-size: 12.5px;
  }
  .crumbs button:hover {
    background: var(--fill-2);
  }
  .crumbs button.last {
    color: var(--label);
    font-weight: 600;
  }
  .list {
    flex: 1;
    min-height: 180px;
    max-height: 300px;
    overflow-y: auto;
    border-radius: var(--r-lg);
    background: var(--surface);
    box-shadow: var(--shadow-sm);
    padding: 4px;
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 38px;
    padding: 0 10px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    text-align: left;
    color: var(--label);
  }
  .folder:hover {
    background: var(--fill-3);
  }
  .folder > :global(svg:first-child) {
    color: var(--accent);
  }
  .folder > :global(svg:last-child) {
    color: var(--label-3);
  }
  .folder span {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .folder.new .field {
    height: 28px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
  }
  .grow {
    flex: 1;
  }
  .note {
    margin: 8px 2px 0;
    color: var(--label-3);
    font-size: 11.5px;
    text-align: right;
  }
</style>
