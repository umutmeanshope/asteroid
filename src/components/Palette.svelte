<script lang="ts" module>
  import type { IconName } from "./Icon.svelte";

  export interface PaletteItem {
    id: string;
    title: string;
    subtitle?: string;
    icon?: IconName;
    iconClass?: string;
    shortcut?: string;
    run: (e: { newTab: boolean }) => void;
  }
</script>

<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { isMac } from "../lib/util";
  import Icon from "./Icon.svelte";
  import Sheet from "./Sheet.svelte";

  let {
    placeholder,
    items,
    query = $bindable(""),
    footer,
  }: {
    placeholder: string;
    items: PaletteItem[];
    query?: string;
    footer?: string;
  } = $props();

  let index = $state(0);
  let list: HTMLDivElement | undefined = $state();
  let input: HTMLInputElement | undefined = $state();

  $effect(() => {
    input?.focus();
  });

  $effect(() => {
    void query;
    index = 0;
  });

  $effect(() => {
    list?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
  });

  function choose(i: number, newTab = false) {
    const item = items[i];
    if (!item) return;
    app.modal = null;
    item.run({ newTab });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "n")) {
      e.preventDefault();
      index = Math.min(items.length - 1, index + 1);
    } else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "p")) {
      e.preventDefault();
      index = Math.max(0, index - 1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      choose(index, isMac ? e.metaKey : e.ctrlKey);
    }
  }
</script>

<Sheet label={placeholder} onclose={() => (app.modal = null)} width={620}>
  <div class="search">
    <Icon name="search" size={18} />
    <input bind:this={input} bind:value={query} {placeholder} onkeydown={onKeydown} spellcheck="false" />
  </div>
  <div class="list" bind:this={list}>
    {#each items as item, i (item.id)}
      <button
        class="item"
        class:on={i === index}
        data-index={i}
        onmousemove={() => (index = i)}
        onclick={(e) => choose(i, isMac ? e.metaKey : e.ctrlKey)}
      >
        {#if item.icon}<span class="icon {item.iconClass ?? ''}"><Icon name={item.icon} size={16} /></span>{/if}
        <span class="text">
          <span class="title">{item.title}</span>
          {#if item.subtitle}<span class="subtitle">{item.subtitle}</span>{/if}
        </span>
        {#if item.shortcut}<span class="kbd">{item.shortcut}</span>{/if}
      </button>
    {:else}
      <div class="empty">No matches</div>
    {/each}
  </div>
  {#if footer}<div class="footer">{footer}</div>{/if}
</Sheet>

<style>
  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 18px;
    height: 54px;
    border-bottom: 0.5px solid var(--separator);
    color: var(--label-3);
    flex: none;
  }
  .search input {
    flex: 1;
    height: 100%;
    border: 0;
    outline: none;
    background: transparent;
    font-size: 17px;
    color: var(--label);
    user-select: text;
    -webkit-user-select: text;
  }
  .search input::placeholder {
    color: var(--label-3);
  }
  .list {
    overflow-y: auto;
    padding: 6px;
    max-height: 420px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 40px;
    padding: 6px 12px;
    border: 0;
    border-radius: var(--r);
    background: transparent;
    text-align: left;
  }
  .item.on {
    background: var(--accent);
    color: #fff;
  }
  .icon {
    color: var(--label-2);
    flex: none;
  }
  .icon.pdf {
    color: var(--red);
  }
  .icon.image {
    color: var(--teal);
  }
  .icon.accent {
    color: var(--accent);
  }
  .item.on .icon {
    color: #fff;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .title {
    font-size: 13.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .subtitle {
    color: var(--label-3);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .item.on .subtitle {
    color: rgba(255, 255, 255, 0.7);
  }
  .item.on .kbd {
    background: rgba(255, 255, 255, 0.2);
    color: #fff;
  }
  .empty {
    padding: 24px;
    color: var(--label-3);
    text-align: center;
  }
  .footer {
    padding: 8px 18px;
    border-top: 0.5px solid var(--separator);
    color: var(--label-3);
    font-size: 11.5px;
    flex: none;
  }
</style>
