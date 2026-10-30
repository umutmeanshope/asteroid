<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { api, message } from "../lib/api";
  import { createEditor, type EditorHandle } from "../lib/editor/setup";
  import { app } from "../lib/state.svelte";
  import type { Tab } from "../lib/types";
  import { copyText } from "../lib/clipboard";
  import { renderMathIn } from "../lib/math";
  import { showMenu, type MenuItem } from "../lib/menu.svelte";
  import { isMac, parentOf, shortcutLabel, stem, vaultBlobUrl } from "../lib/util";
  import Icon from "./Icon.svelte";

  let { tab, active }: { tab: Tab; active: boolean } = $props();

  // This component is keyed by path, so the path is fixed for its lifetime.
  const path = untrack(() => tab.path);
  const tabId = untrack(() => tab.id);

  let loaded = $state(false);
  let error = $state<string | null>(null);
  let html = $state("");
  let editor = $state.raw<EditorHandle | null>(null);
  let editorEl: HTMLDivElement | undefined = $state();
  let readingEl: HTMLElement | undefined = $state();
  let titleEl: HTMLInputElement | undefined = $state();

  let text = "";
  let lastSaved = "";
  let dirty = false;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let saving: Promise<void> | null = null;
  let liveTimer: ReturnType<typeof setTimeout> | undefined;

  async function save() {
    clearTimeout(saveTimer);
    if (!dirty) return saving ?? undefined;
    dirty = false;
    const snapshot = text;
    saving = api
      .writeNote(path, snapshot)
      .then(() => {
        lastSaved = snapshot;
        app.syncSoon();
      })
      .catch((e) => {
        dirty = true;
        app.toast(`Could not save ${stem(path)}: ${message(e)}`, "error");
      });
    await saving;
  }

  function onChange(doc: string) {
    text = doc;
    dirty = true;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(save, 500);
    clearTimeout(liveTimer);
    liveTimer = setTimeout(() => (app.liveText[tabId] = text), 250);
  }

  async function reloadFromDisk() {
    if (dirty) return;
    try {
      const disk = await api.readNote(path);
      if (disk === text) return;
      text = disk;
      lastSaved = disk;
      editor?.setDoc(disk);
      app.liveText[tabId] = disk;
      if (tab.mode === "read") void renderNow();
    } catch {
      /* the file was removed; the tree refresh handles it */
    }
  }

  async function renderNow() {
    try {
      html = await api.renderNote(path, text);
      await tick();
      readingEl?.querySelectorAll<HTMLImageElement>("img[data-vault-src]").forEach((img) => {
        vaultBlobUrl(img.dataset.vaultSrc!, api.readFile).then((url) => (img.src = url)).catch(() => {});
      });
      if (readingEl) renderMathIn(readingEl);
    } catch (e) {
      html = "";
      app.toast(message(e), "error");
    }
  }

  async function resolveImage(target: string): Promise<string | null> {
    const resolved = await api.resolveLink(target, path).catch(() => null);
    return resolved ? vaultBlobUrl(resolved, api.readFile).catch(() => null) : null;
  }

  onMount(() => {
    let disposed = false;
    const unregister = app.registerFlusher(save);
    const unsubscribe = app.onFilesChanged((paths) => {
      if (paths.includes(path)) void reloadFromDisk();
    });

    (async () => {
      try {
        text = await api.readNote(path);
        lastSaved = text;
      } catch (e) {
        error = message(e);
        return;
      }
      if (disposed) return;
      loaded = true;
      await tick();
      editor = createEditor(editorEl!, {
        doc: text,
        livePreview: app.settings.livePreview,
        spellcheck: app.settings.spellcheck,
        getNotes: () => app.notes,
        getTags: () => app.tags,
        onChange,
        onOpenLink: (target, opts) => void app.followLink(target, path, opts),
        resolveImage,
        renderEmbed: (target) => api.renderEmbed(target, path),
      });
      app.liveText[tabId] = text;
      if (tab.mode === "read") void renderNow();
      if (app.focusTitle.has(tabId)) {
        app.focusTitle.delete(tabId);
        titleEl?.focus();
        titleEl?.select();
      } else if (active && tab.mode === "edit") {
        editor.focus();
      }
    })();

    return () => {
      disposed = true;
      void save();
      unregister();
      unsubscribe();
      clearTimeout(liveTimer);
      editor?.destroy();
      delete app.liveText[tabId];
    };
  });

  // Apply editor settings live.
  $effect(() => {
    editor?.configure({ livePreview: app.settings.livePreview, spellcheck: app.settings.spellcheck });
  });

  // Entering reading mode renders the latest text.
  $effect(() => {
    if (tab.mode === "read" && loaded) void save().then(renderNow);
    else if (tab.mode === "edit" && active && editor) editor.focus();
  });

  // One-shot "reveal line / heading" requests from search, outline or links.
  $effect(() => {
    const reveal = tab.reveal;
    if (!reveal || !loaded || !editor) return;
    tab.reveal = undefined;
    void (async () => {
      let line = reveal.line;
      if (reveal.heading) {
        const wanted = reveal.heading.toLowerCase();
        if (tab.mode === "read") {
          await tick();
          const el = [...(readingEl?.querySelectorAll("h1,h2,h3,h4,h5,h6") ?? [])].find(
            (h) => h.textContent?.trim().toLowerCase() === wanted,
          );
          el?.scrollIntoView({ behavior: "smooth", block: "start" });
          return;
        }
        const headings = await api.outline(text);
        line = headings.find((h) => h.text.toLowerCase() === wanted)?.line;
      }
      if (line === undefined) return;
      if (tab.mode === "read") tab.mode = "edit";
      await tick();
      editor?.revealLine(line);
    })();
  });

  async function commitTitle(value: string) {
    const name = value.trim();
    if (!name || name === stem(path)) {
      if (titleEl) titleEl.value = stem(path);
      return;
    }
    await app.rename(path, name);
  }

  /** The link a rendered anchor points at, or null for anything else. */
  function anchorLink(anchor: HTMLAnchorElement): { target: string; external: boolean } | null {
    if (anchor.classList.contains("internal-link")) {
      const href = anchor.dataset.href ?? "";
      const heading = anchor.dataset.heading;
      return { target: heading ? `${href}#${heading}` : href, external: false };
    }
    if (anchor.classList.contains("external-link")) return { target: anchor.getAttribute("href") ?? "", external: true };
    return null;
  }

  function onReadingClick(e: MouseEvent) {
    const anchor = (e.target as HTMLElement).closest("a");
    if (!anchor) return;
    e.preventDefault();
    const link = anchorLink(anchor);
    if (link) void app.followLink(link.target, path, { newTab: !link.external && (isMac ? e.metaKey : e.ctrlKey), external: link.external });
  }

  // Reading view counterpart of the editor menu; Shift+right-click keeps the native one.
  function onReadingContextMenu(e: MouseEvent) {
    if (e.shiftKey) return;
    const anchor = (e.target as HTMLElement).closest("a");
    const link = anchor ? anchorLink(anchor) : null;
    const selection = window.getSelection()?.toString() ?? "";
    const items: MenuItem[] = [];
    if (link) {
      items.push({
        label: link.external ? "Open Link" : "Open Note",
        icon: link.external ? "external" : "note",
        action: () => void app.followLink(link.target, path, { external: link.external }),
      });
      if (!link.external) {
        items.push({ label: "Open in New Tab", icon: "plus", action: () => void app.followLink(link.target, path, { newTab: true }) });
      }
      items.push({ label: "Copy Link", icon: "link", action: () => void copyText(link.target) }, "separator");
    }
    items.push(
      { label: "Copy", icon: "copy", shortcut: shortcutLabel("Mod-c"), disabled: !selection, action: () => void copyText(selection) },
      {
        label: "Select All",
        icon: "select-all",
        shortcut: shortcutLabel("Mod-a"),
        action: () => readingEl && window.getSelection()?.selectAllChildren(readingEl),
      },
      "separator",
      { label: "Edit Note", icon: "pencil", shortcut: shortcutLabel("Mod-e"), action: () => (tab.mode = "edit") },
    );
    showMenu(e, items);
  }
</script>

<div class="note" class:readable={app.settings.readableWidth}>
  {#if error}
    <div class="error">
      <Icon name="warning" size={30} stroke={1.4} />
      <p>{error}</p>
    </div>
  {:else}
    <div class="scroll" class:hidden={tab.mode !== "edit"}>
      <div class="column">
        <div class="meta">
          {#if parentOf(path)}<span class="crumb">{parentOf(path).replaceAll("/", " › ")}</span>{/if}
        </div>
        <input
          bind:this={titleEl}
          class="title"
          value={stem(path)}
          spellcheck="false"
          aria-label="Note title"
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === "ArrowDown") {
              e.preventDefault();
              editor?.focus();
            }
            if (e.key === "Escape") {
              e.currentTarget.value = stem(path);
              editor?.focus();
            }
          }}
          onblur={(e) => void commitTitle(e.currentTarget.value)}
        />
      </div>
      <div class="editor" bind:this={editorEl}></div>
    </div>

    {#if tab.mode === "read"}
      <div class="scroll reading-scroll">
        <div class="column">
          {#if parentOf(path)}<div class="meta"><span class="crumb">{parentOf(path).replaceAll("/", " › ")}</span></div>{/if}
          <h1 class="title static">{stem(path)}</h1>
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
          <article class="markdown" bind:this={readingEl} onclick={onReadingClick} oncontextmenu={onReadingContextMenu}>
            {@html html}
          </article>
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .note {
    position: absolute;
    inset: 0;
    --content-width: none;
  }
  .note.readable {
    --content-width: calc(var(--line-width) + 96px);
  }
  .scroll {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }
  .scroll.hidden {
    visibility: hidden;
  }
  .reading-scroll {
    background: var(--surface);
    animation: fade-in 140ms var(--ease);
  }
  .column {
    width: 100%;
    max-width: var(--content-width);
    margin: 0 auto;
    padding: 24px 50px 0;
  }
  .meta {
    min-height: 16px;
    color: var(--label-3);
    font-size: 12px;
  }
  .title {
    display: block;
    width: 100%;
    margin: 4px 0 10px;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--label);
    font: 700 calc(var(--editor-font-size) * 2) / 1.2 var(--font-display);
    letter-spacing: -0.02em;
    user-select: text;
    -webkit-user-select: text;
  }
  .title::placeholder {
    color: var(--label-3);
  }
  .editor {
    flex: 1;
  }
  .editor :global(.cm-editor) {
    min-height: 100%;
  }
  .editor :global(.cm-scroller) {
    overflow: visible;
  }
  .error {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--label-3);
  }
</style>
