<script lang="ts">
  import { showMenu, type MenuItem } from "../lib/menu.svelte";
  import { app } from "../lib/state.svelte";
  import type { Entry } from "../lib/types";
  import { displayName, isMac } from "../lib/util";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./Icon.svelte";
  import Self from "./TreeNode.svelte";

  let { entry, depth }: { entry: Entry; depth: number } = $props();

  const icons: Record<string, IconName> = { note: "note", pdf: "pdf", image: "image", other: "file", folder: "folder" };

  let isFolder = $derived(entry.kind === "folder");
  let excluded = $derived(app.usesGit && app.isExcluded(entry.path, isFolder));
  let open = $derived(!!app.expanded[entry.path]);
  let isActive = $derived(app.activeTab?.path === entry.path);
  let isSelected = $derived(app.selected === entry.path);
  let renaming = $derived(app.renaming === entry.path);
  let dropTarget = $state(false);
  let input: HTMLInputElement | undefined = $state();

  $effect(() => {
    if (renaming && input) {
      input.focus();
      const name = input.value;
      const dot = isFolder || entry.kind !== "note" ? name.lastIndexOf(".") : -1;
      input.setSelectionRange(0, dot > 0 ? dot : name.length);
    }
  });

  function activate(e: MouseEvent) {
    app.selected = entry.path;
    if (isFolder) {
      app.expanded[entry.path] = !open;
      app.saveWorkspace();
    } else {
      app.openFile(entry.path, { newTab: isMac ? e.metaKey : e.ctrlKey });
    }
  }

  async function commitRename(value: string) {
    if (app.renaming !== entry.path) return;
    app.renaming = null;
    const current = entry.kind === "note" ? displayName(entry.path) : entry.name;
    if (value.trim() && value.trim() !== current) await app.rename(entry.path, value);
  }

  /** "Don't Sync" / "Sync Again" for this entry; type-wide rules are changed in Settings. */
  function syncToggle(): MenuItem {
    const own = app.ignore.paths.includes(isFolder ? `${entry.path}/` : entry.path);
    if (excluded && !own) {
      return {
        label: "Not Synced (File Type)…",
        icon: "cloud-off",
        action: () => {
          app.settingsSection = "sync";
          app.modal = "settings";
        },
      };
    }
    return own
      ? { label: "Sync Again", icon: "sync", action: () => void app.toggleExcluded(entry.path, isFolder) }
      : { label: "Don’t Sync", icon: "cloud-off", action: () => void app.toggleExcluded(entry.path, isFolder) };
  }

  function contextMenu(e: MouseEvent) {
    app.selected = entry.path;
    const folder = isFolder ? entry.path : entry.path.slice(0, Math.max(0, entry.path.lastIndexOf("/")));
    showMenu(e, [
      ...(!isFolder
        ? [{ label: "Open in New Tab", icon: "plus" as const, action: () => app.openFile(entry.path, { newTab: true }) }, "separator" as const]
        : []),
      { label: "New Note", icon: "compose", action: () => void app.newNote(folder) },
      { label: "New Folder", icon: "folder-plus", action: () => void app.newFolder(folder) },
      "separator",
      { label: "Rename…", icon: "pencil", action: () => (app.renaming = entry.path) },
      { label: "Reveal in File Manager", icon: "reveal", action: () => void app.reveal(entry.path) },
      ...(!isFolder && app.vault?.hasHistory
        ? [{ label: "Version History…", icon: "clock" as const, action: () => app.showHistory(entry.path) }]
        : []),
      ...(app.usesGit ? [syncToggle()] : []),
      "separator",
      { label: "Delete…", icon: "trash", destructive: true, action: () => void app.remove(entry.path) },
    ]);
  }
</script>

<div class="node" role="treeitem" aria-selected={isSelected} aria-expanded={isFolder ? open : undefined}>
  <div
    class="item"
    class:active={isActive}
    class:selected={isSelected && !isActive}
    class:drop={dropTarget}
    class:excluded
    style:padding-left="{6 + depth * 14}px"
    draggable={!renaming}
    role="button"
    tabindex="0"
    onclick={activate}
    oncontextmenu={contextMenu}
    onkeydown={(e) => {
      if (e.key === "Enter" && !renaming) app.renaming = entry.path;
      if (e.key === "F2") app.renaming = entry.path;
    }}
    ondragstart={(e) => {
      e.dataTransfer?.setData("application/x-asteroid-path", entry.path);
      if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
    }}
    ondragover={(e) => {
      if (!isFolder || !e.dataTransfer?.types.includes("application/x-asteroid-path")) return;
      e.preventDefault();
      e.stopPropagation();
      dropTarget = true;
    }}
    ondragleave={() => (dropTarget = false)}
    ondrop={(e) => {
      dropTarget = false;
      const path = e.dataTransfer?.getData("application/x-asteroid-path");
      if (!isFolder || !path || path === entry.path || entry.path.startsWith(`${path}/`)) return;
      e.preventDefault();
      e.stopPropagation();
      void app.move(path, entry.path);
    }}
  >
    <span class="chevron" class:open class:invisible={!isFolder}>
      <Icon name="chevron-right" size={11} stroke={2.2} />
    </span>
    <span class="kind {entry.kind}"><Icon name={icons[entry.kind]} size={15} /></span>
    {#if renaming}
      <input
        bind:this={input}
        class="rename"
        value={entry.kind === "note" ? displayName(entry.path) : entry.name}
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => {
          e.stopPropagation();
          if (e.key === "Enter") void commitRename(e.currentTarget.value);
          if (e.key === "Escape") app.renaming = null;
        }}
        onblur={(e) => void commitRename(e.currentTarget.value)}
      />
    {:else}
      <span class="name">{entry.kind === "note" ? displayName(entry.path) : entry.name}</span>
      {#if excluded}<span class="off" title="Not synced"><Icon name="cloud-off" size={12} /></span>{/if}
    {/if}
  </div>

  {#if isFolder && open && entry.children}
    <div class="children">
      {#each entry.children as child (child.path)}
        <Self entry={child} depth={depth + 1} />
      {/each}
    </div>
  {/if}
</div>

<style>
  .item {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding-right: 8px;
    border-radius: 7px;
    color: var(--label);
    font-size: 13px;
    outline: none;
    transition: background 120ms var(--ease);
  }
  .item:hover {
    background: var(--fill-3);
  }
  .item.selected {
    background: var(--fill-2);
  }
  .item.active {
    background: var(--accent-tint-strong);
  }
  .item.drop {
    background: var(--accent-tint-strong);
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .item:focus-visible {
    box-shadow: inset 0 0 0 2px var(--accent-tint-strong);
  }
  .chevron {
    display: grid;
    place-items: center;
    width: 14px;
    color: var(--label-3);
    transition: transform 200ms var(--ease-out);
    flex: none;
  }
  .chevron.open {
    transform: rotate(90deg);
  }
  .chevron.invisible {
    visibility: hidden;
  }
  .kind {
    color: var(--label-2);
    flex: none;
  }
  .kind.folder {
    color: var(--accent);
  }
  .kind.pdf {
    color: var(--red);
  }
  .kind.image {
    color: var(--teal);
  }
  .item.excluded .name,
  .item.excluded .kind {
    opacity: 0.5;
  }
  .off {
    flex: none;
    margin-right: 4px;
    color: var(--label-3);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rename {
    flex: 1;
    min-width: 0;
    height: 22px;
    padding: 0 6px;
    border: 0;
    border-radius: 5px;
    background: var(--surface);
    box-shadow: 0 0 0 2px var(--accent);
    outline: none;
    font-size: 13px;
    user-select: text;
    -webkit-user-select: text;
  }
</style>
