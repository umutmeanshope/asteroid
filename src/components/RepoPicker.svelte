<script lang="ts">
  import { api, message } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import type { GithubRepo, GithubStatus } from "../lib/types";
  import { relativeTime } from "../lib/util";
  import GithubConnect from "./GithubConnect.svelte";
  import Icon from "./Icon.svelte";

  let {
    action,
    onpick,
    busy = false,
    allowCreate = false,
    suggestedName = "",
  }: {
    /** Label for the primary button, given the selected repository. */
    action: (repo: GithubRepo) => string;
    onpick: (repo: GithubRepo) => void;
    busy?: boolean;
    /** Offer creating a new private repository (for linking, not cloning). */
    allowCreate?: boolean;
    suggestedName?: string;
  } = $props();

  let status = $state<GithubStatus | null>(null);
  let repos = $state<GithubRepo[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let query = $state("");
  let selected = $state<GithubRepo | null>(null);
  let connecting = $state(false);
  let newName = $state<string | null>(null);
  let creating = $state(false);

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? repos.filter((r) => r.fullName.toLowerCase().includes(q) || r.description?.toLowerCase().includes(q)) : repos;
  });

  async function refresh() {
    status = await api.githubStatus().catch(() => null);
    if (!status?.connected) return;
    loading = true;
    error = null;
    try {
      repos = await api.githubRepos();
    } catch (e) {
      error = message(e);
    } finally {
      loading = false;
    }
  }
  $effect(() => {
    void refresh();
  });

  /** A GitHub-safe repository name from a vault name. */
  function slug(name: string) {
    return name.trim().replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "") || "notes";
  }

  async function create() {
    const name = newName?.trim();
    if (!name) return;
    creating = true;
    try {
      const repo = await api.githubCreateRepo(slug(name), true);
      repos = [repo, ...repos];
      selected = repo;
      newName = null;
      onpick(repo);
    } catch (e) {
      app.toast(message(e), "error");
    } finally {
      creating = false;
    }
  }
</script>

<div class="picker">
  {#if !status}
    <div class="center"><span class="spinner"></span></div>
  {:else if !status.configured}
    <div class="center">
      <Icon name="git" size={34} stroke={1.3} />
      <p>Add your GitHub OAuth client ID in <strong>Settings › Sync</strong> to use GitHub.</p>
      <button
        class="btn"
        onclick={() => {
          app.settingsSection = "sync";
          app.modal = "settings";
        }}>Open Sync Settings</button
      >
    </div>
  {:else if !status.connected}
    {#if connecting}
      <GithubConnect
        onconnected={(login) => {
          connecting = false;
          app.toast(`Connected to GitHub as ${login}`, "success");
          void refresh();
        }}
        oncancel={() => (connecting = false)}
      />
    {:else}
      <div class="center">
        <Icon name="git" size={34} stroke={1.3} />
        <p>Sign in with GitHub to choose a repository.</p>
        <button class="btn primary" onclick={() => (connecting = true)}>Connect GitHub</button>
      </div>
    {/if}
  {:else}
    <div class="search">
      <Icon name="search" size={14} />
      <input placeholder="Search repositories" bind:value={query} />
    </div>
    <div class="list">
      {#if allowCreate}
        {#if newName === null}
          <button class="repo create" onclick={() => (newName = slug(suggestedName))}>
            <span class="lead"><Icon name="plus" size={16} stroke={2} /></span>
            <span class="text"><strong>New private repository</strong><small>Created on your GitHub account</small></span>
          </button>
        {:else}
          <div class="repo create editing">
            <span class="lead"><Icon name="plus" size={16} stroke={2} /></span>
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="field"
              autofocus
              placeholder="Repository name"
              bind:value={newName}
              onkeydown={(e) => {
                if (e.key === "Enter") void create();
                if (e.key === "Escape") {
                  e.stopPropagation();
                  newName = null;
                }
              }}
            />
            <button class="btn primary" disabled={creating || !newName.trim()} onclick={create}>
              {#if creating}<span class="spinner"></span>{/if} Create & Sync
            </button>
          </div>
        {/if}
      {/if}
      {#if loading}
        <div class="center small"><span class="spinner"></span></div>
      {:else if error}
        <div class="center small error">{error}</div>
      {:else}
        {#each filtered as repo (repo.fullName)}
          <button class="repo" class:selected={selected?.fullName === repo.fullName} onclick={() => (selected = repo)} ondblclick={() => onpick(repo)}>
            <span class="lead"><Icon name={repo.private ? "lock" : "git"} size={16} /></span>
            <span class="text">
              <strong>{repo.fullName}</strong>
              <small>{repo.description || (repo.updatedAt ? `Updated ${relativeTime(Date.parse(repo.updatedAt))}` : repo.defaultBranch)}</small>
            </span>
          </button>
        {:else}
          <div class="center small">{query ? "No matching repositories" : "No repositories yet"}</div>
        {/each}
      {/if}
    </div>
    <div class="actions">
      <span class="account">Signed in as <strong>{status.login}</strong></span>
      <span class="grow"></span>
      <button class="btn primary" disabled={busy || !selected} onclick={() => selected && onpick(selected)}>
        {#if busy}<span class="spinner"></span>{/if}
        {selected ? action(selected) : "Choose a Repository"}
      </button>
    </div>
  {/if}
</div>

<style>
  .picker {
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
  .search {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    margin-bottom: 8px;
    padding: 0 10px;
    border-radius: var(--r-sm);
    background: var(--fill-2);
    color: var(--label-3);
  }
  .search input {
    flex: 1;
    border: 0;
    outline: none;
    background: none;
    color: var(--label);
    font: inherit;
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
  .repo {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 44px;
    padding: 5px 10px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    text-align: left;
    color: var(--label);
  }
  .repo:hover {
    background: var(--fill-3);
  }
  .repo.selected {
    background: var(--accent);
    color: #fff;
  }
  .repo.selected small,
  .repo.selected .lead {
    color: rgba(255, 255, 255, 0.8);
  }
  .lead {
    display: grid;
    place-items: center;
    flex: none;
    width: 20px;
    color: var(--label-2);
  }
  .create .lead {
    color: var(--accent);
  }
  .create.editing .field {
    flex: 1;
    height: 28px;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .text strong,
  .text small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .text strong {
    font-weight: 500;
  }
  .text small {
    color: var(--label-2);
    font-size: 11.5px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
  }
  .account {
    color: var(--label-3);
    font-size: 12px;
  }
  .grow {
    flex: 1;
  }
</style>
