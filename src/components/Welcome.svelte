<script lang="ts">
  import { onMount } from "svelte";
  import { documentDir } from "@tauri-apps/api/path";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, message } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import type { RecentVault } from "../lib/types";
  import { relativeTime } from "../lib/util";
  import Icon from "./Icon.svelte";

  let recent = $state<RecentVault[]>([]);
  let creating = $state(false);
  let name = $state("My Notes");
  let location = $state("");
  let busy = $state(false);

  onMount(async () => {
    recent = await api.recentVaults().catch(() => []);
    location = await documentDir().catch(() => "");
  });

  async function chooseLocation() {
    const dir = await openDialog({ directory: true, title: "Choose where to create the vault", defaultPath: location || undefined });
    if (typeof dir === "string") location = dir;
  }

  async function create() {
    if (!name.trim() || !location) return;
    busy = true;
    try {
      await app.createVault(location, name.trim());
    } catch (e) {
      app.toast(message(e), "error");
    } finally {
      busy = false;
    }
  }

  async function openRecent(v: RecentVault) {
    try {
      await app.openVault(v.path);
    } catch (e) {
      app.toast(message(e), "error");
      await api.forgetRecentVault(v.path);
      recent = recent.filter((r) => r.path !== v.path);
    }
  }
</script>

<div class="welcome" data-tauri-drag-region>
  <div class="card glass">
    <div class="intro">
      <img src="/icon.svg" alt="" width="88" height="88" />
      <h1>Asteroid</h1>
      <p>Your notes, as plain Markdown files.</p>
    </div>

    {#if creating}
      <div class="create">
        <label class="lbl" for="vault-name">Vault name</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="vault-name" class="field" bind:value={name} autofocus onkeydown={(e) => e.key === "Enter" && create()} />
        <span class="lbl">Location</span>
        <button class="location" onclick={chooseLocation}>
          <Icon name="folder" size={15} />
          <span>{location || "Choose a folder…"}</span>
          <span class="change">Change…</span>
        </button>
        <div class="buttons">
          <button class="btn" onclick={() => (creating = false)}>Cancel</button>
          <button class="btn primary" disabled={busy || !name.trim() || !location} onclick={create}>Create Vault</button>
        </div>
      </div>
    {:else}
      <div class="actions">
        <button class="action" onclick={() => (creating = true)}>
          <span class="tile" style:background="linear-gradient(160deg,#4da3ff,#007aff)"><Icon name="plus" size={18} stroke={2} /></span>
          <span class="text"><strong>Create new vault</strong><small>Start fresh in a new folder</small></span>
          <Icon name="chevron-right" size={13} stroke={2} />
        </button>
        <button class="action" onclick={() => app.pickAndOpenVault()}>
          <span class="tile" style:background="linear-gradient(160deg,#5ac8fa,#0a84ff)"><Icon name="folder" size={18} stroke={1.8} /></span>
          <span class="text"><strong>Open folder as vault</strong><small>Works with existing Obsidian vaults</small></span>
          <Icon name="chevron-right" size={13} stroke={2} />
        </button>
        <button class="action" onclick={() => (app.modal = "drive-pull")}>
          <span class="tile" style:background="linear-gradient(160deg,#34c759,#0a9b41)"><Icon name="drive" size={18} stroke={1.7} /></span>
          <span class="text"><strong>Open from Google Drive</strong><small>Download a vault and keep it in sync</small></span>
          <Icon name="chevron-right" size={13} stroke={2} />
        </button>
        <button class="action" onclick={() => (app.modal = "git-pull")}>
          <span class="tile" style:background="linear-gradient(160deg,#48484a,#1c1c1e)"><Icon name="git" size={18} stroke={1.8} /></span>
          <span class="text"><strong>Open from GitHub</strong><small>Clone a repository of notes and keep it in sync</small></span>
          <Icon name="chevron-right" size={13} stroke={2} />
        </button>
      </div>

      {#if recent.length}
        <div class="recent">
          <div class="group-title">Recent</div>
          <div class="group">
            {#each recent as v (v.path)}
              <button class="row recent-row" onclick={() => openRecent(v)} title={v.path}>
                <Icon name="vault" size={16} />
                <span class="label">{v.name}<small>{v.path}</small></span>
                <span class="when">{relativeTime(v.openedMs)}</span>
              </button>
            {/each}
          </div>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .welcome {
    height: 100%;
    display: grid;
    place-items: center;
    padding: 24px;
    overflow-y: auto;
    background:
      radial-gradient(1200px 600px at 15% 10%, color-mix(in srgb, var(--accent) 16%, transparent), transparent 60%),
      radial-gradient(900px 600px at 90% 90%, color-mix(in srgb, var(--purple) 14%, transparent), transparent 60%),
      var(--window);
  }
  .card {
    width: min(460px, 100%);
    padding: 34px 28px 26px;
    border-radius: 28px;
    background: var(--glass-strong);
    animation: sheet-in 420ms var(--ease-out);
  }
  .intro {
    text-align: center;
    margin-bottom: 26px;
  }
  .intro img {
    filter: drop-shadow(0 10px 24px rgba(0, 0, 0, 0.22));
  }
  h1 {
    margin: 12px 0 4px;
    font: 700 28px var(--font-display);
    letter-spacing: -0.02em;
  }
  .intro p {
    margin: 0;
    color: var(--label-2);
    font-size: 14px;
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .action {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 14px 10px 10px;
    border: 0;
    border-radius: 16px;
    background: var(--surface);
    box-shadow: var(--shadow-sm);
    text-align: left;
    transition: transform 150ms var(--ease), box-shadow 150ms var(--ease);
  }
  .action:hover {
    box-shadow: var(--shadow);
    transform: translateY(-1px);
  }
  .action:active {
    transform: scale(0.99);
  }
  .action > :global(svg:last-child) {
    color: var(--label-3);
  }
  .tile {
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    border-radius: 10px;
    color: #fff;
    flex: none;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.18);
  }
  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  .text strong {
    font-size: 14px;
    font-weight: 600;
  }
  .text small {
    color: var(--label-2);
    font-size: 12px;
  }
  .recent {
    margin-top: 6px;
  }
  .recent-row {
    width: 100%;
    border: 0;
    background: transparent;
    text-align: left;
  }
  .recent-row:hover {
    background: var(--fill-3);
  }
  .recent-row > :global(svg) {
    color: var(--accent);
  }
  .recent-row small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .when {
    color: var(--label-3);
    font-size: 11.5px;
    white-space: nowrap;
  }
  .create {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .lbl {
    margin-top: 6px;
    color: var(--label-2);
    font-size: 12px;
    font-weight: 500;
  }
  .location {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-sm);
    background: var(--fill-2);
    text-align: left;
  }
  .location > :global(svg) {
    color: var(--accent);
  }
  .location span:not(.change) {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .change {
    color: var(--accent);
    font-size: 12px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
