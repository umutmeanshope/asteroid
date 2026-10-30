<script lang="ts">
  import { api } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import type { Backlink, Heading } from "../lib/types";
  import { debounce, parentOf } from "../lib/util";
  import Icon from "./Icon.svelte";

  let view = $state<"outline" | "backlinks">("outline");
  let headings = $state<Heading[]>([]);
  let backlinks = $state<Backlink[]>([]);

  let tab = $derived(app.activeTab);

  const loadOutline = debounce(async (text: string) => {
    headings = await api.outline(text).catch(() => []);
  }, 200);

  $effect(() => {
    const text = tab ? app.liveText[tab.id] : undefined;
    if (text !== undefined) loadOutline(text);
    else headings = [];
  });

  async function loadBacklinks(path: string) {
    backlinks = await api.backlinks(path).catch(() => []);
  }

  $effect(() => {
    // Refresh when the note changes or the index updates.
    void app.notes;
    if (tab) void loadBacklinks(tab.path);
  });

  let minLevel = $derived(headings.length ? Math.min(...headings.map((h) => h.level)) : 1);
</script>

<aside class="panel glass">
  <div class="head">
    <div class="segmented">
      <button class:on={view === "outline"} onclick={() => (view = "outline")}>Outline</button>
      <button class:on={view === "backlinks"} onclick={() => (view = "backlinks")}>
        Backlinks{#if backlinks.length}<span class="badge">{backlinks.length}</span>{/if}
      </button>
    </div>
  </div>

  <div class="body">
    {#if view === "outline"}
      {#each headings as h, i (i)}
        <button
          class="heading"
          style:padding-left="{10 + (h.level - minLevel) * 14}px"
          class:top={h.level === minLevel}
          onclick={() => tab && (tab.reveal = { line: h.line, nonce: Date.now() })}
        >
          {h.text}
        </button>
      {:else}
        <p class="hint"><Icon name="outline" size={22} stroke={1.4} />Headings in this note appear here.</p>
      {/each}
    {:else}
      {#each backlinks as link, i (i)}
        <button class="backlink" onclick={() => app.openFile(link.source, { line: link.line })}>
          <div class="source">
            <Icon name="note" size={13} />
            <span>{link.title}</span>
            {#if parentOf(link.source)}<small>{parentOf(link.source)}</small>{/if}
          </div>
          <div class="context">{link.context}</div>
        </button>
      {:else}
        <p class="hint"><Icon name="link" size={22} stroke={1.4} />No other notes link here yet.</p>
      {/each}
    {/if}
  </div>
</aside>

<style>
  .panel {
    position: absolute;
    z-index: 5;
    top: var(--gutter);
    bottom: var(--gutter);
    right: var(--gutter);
    width: 280px;
    display: flex;
    flex-direction: column;
    border-radius: 18px;
    animation: panel-in 300ms var(--ease-out);
  }
  @keyframes panel-in {
    from {
      opacity: 0;
      transform: translateX(16px);
    }
  }
  .head {
    padding: 10px;
  }
  .segmented {
    display: flex;
  }
  .badge {
    margin-left: 5px;
    padding: 0 6px;
    border-radius: var(--r-pill);
    background: var(--accent);
    color: #fff;
    font-size: 10px;
  }
  .body {
    flex: 1;
    overflow-y: auto;
    padding: 0 8px 12px;
  }
  .heading {
    display: block;
    width: 100%;
    padding: 5px 10px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--label-2);
    font-size: 12.5px;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .heading.top {
    color: var(--label);
    font-weight: 600;
  }
  .heading:hover {
    background: var(--fill-3);
    color: var(--label);
  }
  .backlink {
    display: block;
    width: 100%;
    margin-bottom: 6px;
    padding: 9px 11px;
    border: 0;
    border-radius: 11px;
    background: var(--surface);
    box-shadow: var(--shadow-sm);
    text-align: left;
  }
  .backlink:hover {
    box-shadow: var(--shadow);
  }
  .source {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--label);
    font-weight: 600;
    font-size: 12.5px;
  }
  .source :global(svg) {
    color: var(--accent);
  }
  .source small {
    color: var(--label-3);
    font-weight: 400;
    margin-left: auto;
  }
  .context {
    margin-top: 4px;
    color: var(--label-2);
    font-size: 12px;
    line-height: 1.45;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .hint {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    margin: 32px 16px;
    color: var(--label-3);
    text-align: center;
    font-size: 12px;
  }
</style>
