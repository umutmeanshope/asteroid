<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { fuzzyScore, modKey } from "../lib/util";
  import Palette, { type PaletteItem } from "./Palette.svelte";

  let query = $state("");

  const commands: PaletteItem[] = [
    { id: "new-note", title: "New note", icon: "compose", shortcut: `${modKey} N`, run: () => void app.newNote() },
    { id: "new-folder", title: "New folder", icon: "folder-plus", shortcut: `${modKey} ⇧ N`, run: () => void app.newFolder() },
    { id: "open", title: "Open note…", icon: "search", shortcut: `${modKey} O`, run: () => setTimeout(() => (app.modal = "switcher")) },
    { id: "toggle-mode", title: "Toggle reading view", icon: "book", shortcut: `${modKey} E`, run: () => app.toggleMode() },
    {
      id: "search",
      title: "Search vault",
      icon: "search",
      shortcut: `${modKey} ⇧ F`,
      run: () => {
        app.sidebarOpen = true;
        app.sidebarView = "search";
      },
    },
    { id: "sidebar", title: "Toggle sidebar", icon: "sidebar", shortcut: `${modKey} \\`, run: () => (app.sidebarOpen = !app.sidebarOpen) },
    { id: "right", title: "Toggle outline & backlinks", icon: "sidebar-right", run: () => (app.rightOpen = !app.rightOpen) },
    { id: "live", title: "Toggle live preview", icon: "sparkle", run: () => void app.updateSettings({ livePreview: !app.settings.livePreview }) },
    { id: "sync", title: app.vault?.sync?.provider === "git" ? "Sync with GitHub" : "Sync with Google Drive", icon: "sync", run: () => void app.syncNow() },
    { id: "close-tab", title: "Close tab", icon: "xmark", shortcut: `${modKey} W`, run: () => app.activeId && app.closeTab(app.activeId) },
    {
      id: "settings",
      title: "Settings",
      icon: "gear",
      shortcut: `${modKey} ,`,
      run: () => setTimeout(() => (app.modal = "settings")),
    },
    { id: "switch-vault", title: "Open another vault…", icon: "vault", run: () => void app.pickAndOpenVault() },
    { id: "pull", title: "Open vault from Google Drive…", icon: "drive", run: () => setTimeout(() => (app.modal = "drive-pull")) },
    { id: "close-vault", title: "Close vault", icon: "arrow-left", run: () => void app.closeVault() },
    ...(app.vault?.hasHistory
      ? [
          ...(app.activeTab
            ? [{ id: "history", title: "Show version history", icon: "clock" as const, run: () => setTimeout(() => app.showHistory(app.activeTab!.path)) }]
            : []),
          { id: "deleted", title: "Recently deleted files…", icon: "trash" as const, run: () => setTimeout(() => app.showHistory(null)) },
        ]
      : []),
  ];

  let items = $derived(
    query.trim()
      ? commands
          .map((c) => ({ c, s: fuzzyScore(query.trim(), c.title) }))
          .filter((r) => r.s > 0)
          .sort((a, b) => b.s - a.s)
          .map((r) => r.c)
      : commands,
  );
</script>

<Palette placeholder="Type a command…" bind:query {items} />
