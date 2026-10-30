<script lang="ts">
  import { api } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import type { SearchHit } from "../lib/types";
  import { debounce, parentOf } from "../lib/util";
  import Icon from "./Icon.svelte";

  let hits = $state<SearchHit[]>([]);
  let searching = $state(false);
  let input: HTMLInputElement | undefined = $state();

  const run = debounce(async (query: string) => {
    if (!query.trim()) {
      hits = [];
      searching = false;
      return;
    }
    try {
      hits = await api.search(query);
    } finally {
      searching = false;
    }
  }, 160);

  $effect(() => {
    const q = app.searchQuery;
    searching = !!q.trim();
    run(q);
  });

  // Re-run when the vault changes so results stay fresh.
  $effect(() => app.onFilesChanged(() => run(app.searchQuery)));

  $effect(() => {
    input?.focus();
  });

  function terms(): string[] {
    return app.searchQuery
      .split(/\s+/)
      .filter((t) => t && !t.startsWith("#") && !t.startsWith("tag:"))
      .map((t) => t.toLowerCase());
  }

  /** Splits a snippet into plain and highlighted parts without using HTML. */
  function highlight(text: string): { text: string; hit: boolean }[] {
    const ts = terms();
    if (!ts.length) return [{ text, hit: false }];
    const lower = text.toLowerCase();
    const parts: { text: string; hit: boolean }[] = [];
    let i = 0;
    while (i < text.length) {
      let best = -1;
      let len = 0;
      for (const t of ts) {
        const at = lower.indexOf(t, i);
        if (at >= 0 && (best < 0 || at < best)) {
          best = at;
          len = t.length;
        }
      }
      if (best < 0) {
        parts.push({ text: text.slice(i), hit: false });
        break;
      }
      if (best > i) parts.push({ text: text.slice(i, best), hit: false });
      parts.push({ text: text.slice(best, best + len), hit: true });
      i = best + len;
    }
    return parts;
  }
</script>

<div class="search">
  <div class="box">
    <Icon name="search" size={14} />
    <input
      bind:this={input}
      class="field"
      placeholder="Search notes or #tag"
      bind:value={app.searchQuery}
      onkeydown={(e) => {
        if (e.key === "Escape") app.searchQuery = "";
        if (e.key === "Enter" && hits[0]) app.openFile(hits[0].path, { line: hits[0].line ?? undefined });
      }}
    />
    {#if searching}<span class="spinner"></span>{/if}
  </div>

  <div class="results">
    {#if app.searchQuery.trim() && !searching}
      <div class="count">{hits.length} {hits.length === 1 ? "result" : "results"}</div>
    {/if}
    {#each hits as hit (hit.path)}
      <button
        class="hit"
        class:active={app.activeTab?.path === hit.path}
        onclick={() => app.openFile(hit.path, { line: hit.line ?? undefined })}
      >
        <div class="title">{hit.title}</div>
        {#if parentOf(hit.path)}<div class="folder">{parentOf(hit.path)}</div>{/if}
        <div class="snippet">
          {#each highlight(hit.snippet) as part}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
        </div>
      </button>
    {/each}
  </div>
</div>

<style>
  .search {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .box {
    position: relative;
    display: flex;
    align-items: center;
    margin: 0 8px 8px;
  }
  .box > :global(svg) {
    position: absolute;
    left: 9px;
    color: var(--label-3);
    pointer-events: none;
  }
  .box .field {
    padding-left: 28px;
    padding-right: 28px;
  }
  .box .spinner {
    position: absolute;
    right: 9px;
  }
  .results {
    flex: 1;
    overflow-y: auto;
    padding: 0 8px 12px;
  }
  .count {
    padding: 2px 8px 6px;
    color: var(--label-3);
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .hit {
    display: block;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    text-align: left;
  }
  .hit:hover {
    background: var(--fill-3);
  }
  .hit.active {
    background: var(--accent-tint);
  }
  .title {
    font-weight: 600;
    font-size: 13px;
    color: var(--label);
  }
  .folder {
    color: var(--label-3);
    font-size: 11px;
  }
  .snippet {
    margin-top: 2px;
    color: var(--label-2);
    font-size: 12px;
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  mark {
    background: var(--mark);
    color: inherit;
    border-radius: 3px;
    padding: 0 1px;
  }
</style>
