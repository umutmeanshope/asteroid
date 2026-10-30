<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { app } from "./lib/state.svelte";
  import { isMac } from "./lib/util";
  import Welcome from "./components/Welcome.svelte";
  import Workspace from "./components/Workspace.svelte";
  import QuickSwitcher from "./components/QuickSwitcher.svelte";
  import CommandPalette from "./components/CommandPalette.svelte";
  import Settings from "./components/Settings.svelte";
  import PullSheet from "./components/PullSheet.svelte";
  import GitPullSheet from "./components/GitPullSheet.svelte";
  import HistorySheet from "./components/HistorySheet.svelte";
  import Toasts from "./components/Toasts.svelte";
  import ContextMenu from "./components/ContextMenu.svelte";
  import Icon from "./components/Icon.svelte";

  let dropActive = $state(false);

  function onKeydown(e: KeyboardEvent) {
    const mod = isMac ? e.metaKey : e.ctrlKey;
    const key = e.key.toLowerCase();

    if (mod && key === ",") {
      e.preventDefault();
      app.modal = app.modal === "settings" ? null : "settings";
      return;
    }
    if (!app.vault) return;

    if (e.ctrlKey && key === "tab") {
      e.preventDefault();
      app.cycleTab(e.shiftKey ? -1 : 1);
      return;
    }
    if (!mod) return;

    const run = (fn: () => void) => {
      e.preventDefault();
      e.stopPropagation();
      fn();
    };
    if (key === "o" && !e.shiftKey) run(() => (app.modal = app.modal === "switcher" ? null : "switcher"));
    else if (key === "p" && !e.shiftKey) run(() => (app.modal = app.modal === "palette" ? null : "palette"));
    else if (key === "n" && e.shiftKey) run(() => void app.newFolder());
    else if (key === "n") run(() => void app.newNote());
    else if (key === "e") run(() => app.toggleMode());
    else if (key === "w" && app.activeId) run(() => app.closeTab(app.activeId!));
    else if (key === "f" && e.shiftKey)
      run(() => {
        app.sidebarOpen = true;
        app.sidebarView = "search";
      });
    else if (key === "\\") run(() => (app.sidebarOpen = !app.sidebarOpen));
    else if ((key === "[" || (e.altKey && key === "arrowleft")) && !e.shiftKey) run(() => app.back());
    else if ((key === "]" || (e.altKey && key === "arrowright")) && !e.shiftKey) run(() => app.forward());
  }

  /** Dev-only: `?scenario=` opens a view directly, for visual checks. */
  async function devScenario() {
    const scenario = new URLSearchParams(location.search).get("scenario");
    if (!scenario || !app.vault) return;
    for (const step of scenario.split(",")) {
      const [cmd, arg] = step.split(":");
      if (cmd === "open" && arg) app.openFile(decodeURIComponent(arg), { newTab: true });
      else if (cmd === "read") app.setMode("read");
      else if (cmd === "right") app.rightOpen = true;
      else if (cmd === "dark") app.settings.theme = "dark";
      else if (cmd === "modal") app.modal = arg as typeof app.modal;
      else if (cmd === "section") app.settingsSection = arg as typeof app.settingsSection;
      else if (cmd === "history") app.showHistory(arg ? decodeURIComponent(arg) : null);
      else if (cmd === "search") {
        app.sidebarView = "search";
        app.searchQuery = decodeURIComponent(arg ?? "");
      }
      app.applyAppearance();
      await new Promise((r) => setTimeout(r, 150));
    }
  }

  onMount(() => {
    void app.init().then(() => {
      if (import.meta.env.DEV) void devScenario();
    });
    document.documentElement.classList.toggle("mac", isMac);

    const unlisteners: Promise<() => void>[] = [
      getCurrentWebview().onDragDropEvent((event) => {
        const p = event.payload;
        if (!app.vault) return;
        if (p.type === "enter" || p.type === "over") dropActive = true;
        else if (p.type === "leave") dropActive = false;
        else if (p.type === "drop") {
          dropActive = false;
          if (p.paths.length) void app.importFiles(p.paths);
        }
      }),
      getCurrentWindow().onCloseRequested(async () => {
        await app.flushAll();
        app.saveWorkspace();
      }),
    ];
    return () => unlisteners.forEach((u) => u.then((f) => f()));
  });
</script>

<svelte:window onkeydown={onKeydown} />

{#if app.ready}
  {#if app.vault}
    <Workspace />
  {:else}
    <Welcome />
  {/if}
{/if}

{#if app.modal === "switcher"}
  <QuickSwitcher />
{:else if app.modal === "palette"}
  <CommandPalette />
{:else if app.modal === "settings"}
  <Settings />
{:else if app.modal === "drive-pull"}
  <PullSheet />
{:else if app.modal === "git-pull"}
  <GitPullSheet />
{:else if app.modal === "history"}
  <HistorySheet />
{/if}

{#if dropActive}
  <div class="drop">
    <div class="drop-card glass">
      <Icon name="cloud-down" size={34} stroke={1.4} />
      <strong>Drop to add to vault</strong>
      <span>PDFs, images and Markdown files are copied in.</span>
    </div>
  </div>
{/if}

{#if app.opening}
  <div class="opening">
    <div class="opening-card glass">
      <div class="spinner"></div>
      <span>Opening “{app.opening}”…</span>
    </div>
  </div>
{/if}

<ContextMenu />
<Toasts />

<style>
  /* Blocks input during a vault switch; fades in late so fast opens don't flash. */
  .opening {
    position: fixed;
    inset: 0;
    z-index: 95;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--window) 40%, transparent);
    animation: fade-in 200ms 200ms var(--ease) both;
  }
  .opening-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 20px;
    border-radius: var(--r-xl);
    font-size: 13px;
    color: var(--label-2);
  }
  .drop {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    border: 2px dashed color-mix(in srgb, var(--accent) 55%, transparent);
    border-radius: 14px;
    margin: 6px;
    pointer-events: none;
    animation: fade-in 120ms var(--ease);
  }
  .drop-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 26px 34px;
    border-radius: var(--r-xl);
    color: var(--accent);
  }
  .drop-card strong {
    color: var(--label);
    font-size: 15px;
  }
  .drop-card span {
    color: var(--label-2);
  }
</style>
