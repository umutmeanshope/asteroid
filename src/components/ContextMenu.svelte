<script lang="ts">
  import { tick } from "svelte";
  import { closeMenu, menu, type MenuItem } from "../lib/menu.svelte";
  import Icon from "./Icon.svelte";

  const MARGIN = 8;
  /** Grace period before hovering another item closes an open submenu, so
   *  the pointer can travel diagonally into it. */
  const SUBMENU_GRACE = 220;

  let el: HTMLDivElement | undefined = $state();
  let subEl: HTMLDivElement | undefined = $state();
  let pos = $state({ left: 0, top: 0 });
  let active = $state(-1);
  /** The open submenu: which top-level item, and where it's drawn. A size
   *  picker submenu shows a columns × rows grid instead of items. */
  let sub = $state.raw<{
    index: number;
    items: MenuItem[];
    picker?: (cols: number, rows: number) => void;
    left: number;
    top: number;
  } | null>(null);
  const GRID = 8;
  let pick = $state({ cols: 0, rows: 0 });
  let subActive = $state(-1);
  /** Which level the keyboard moves in. */
  let level = $state<0 | 1>(0);
  let closeTimer: ReturnType<typeof setTimeout> | undefined;
  /** Focused element before the menu opened, restored when it's dismissed by key. */
  let returnFocus: HTMLElement | null = null;
  let open = $derived(menu.current);

  type Entry = Exclude<MenuItem, "separator">;
  const enabled = (item: MenuItem): item is Entry => item !== "separator" && !item.disabled;

  // Place at the requested point, then keep the whole menu on screen: shift
  // left at the right edge, flip upwards at the bottom edge.
  $effect(() => {
    const m = open;
    sub = null;
    level = 0;
    if (!m) return;
    active = -1;
    returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    pos = { left: m.x, top: m.y };
    void tick().then(() => {
      if (!el || menu.current !== m) return;
      const { width, height } = el.getBoundingClientRect();
      const left = Math.min(m.x, innerWidth - MARGIN - width);
      let top = m.y;
      if (top + height > innerHeight - MARGIN) top = m.flipY - height;
      pos = { left: Math.max(MARGIN, left), top: Math.max(MARGIN, top) };
      el.focus({ preventScroll: true });
    });
  });

  /** Opens item `index`'s submenu beside it; on the left if the right is full. */
  async function openSub(index: number, focusFirst = false) {
    clearTimeout(closeTimer);
    const item = open?.items[index];
    if (!item || item === "separator" || (!item.submenu && !item.sizePicker) || item.disabled) return;
    if (sub?.index === index) {
      if (focusFirst) focusSub();
      return;
    }
    const row = el?.querySelector<HTMLElement>(`[data-index="${index}"]`)?.getBoundingClientRect();
    if (!row) return;
    sub = { index, items: item.submenu ?? [], picker: item.sizePicker, left: row.right - 4, top: row.top - 5 };
    subActive = -1;
    pick = { cols: 0, rows: 0 };
    await tick();
    if (!subEl || sub?.index !== index) return;
    const { width, height } = subEl.getBoundingClientRect();
    let left = row.right - 4;
    if (left + width > innerWidth - MARGIN) left = row.left - width + 4;
    const top = Math.min(row.top - 5, innerHeight - MARGIN - height);
    sub = { ...sub, left: Math.max(MARGIN, left), top: Math.max(MARGIN, top) };
    if (focusFirst) focusSub();
  }

  function focusSub() {
    level = 1;
    if (sub?.picker) pick = { cols: Math.max(1, pick.cols), rows: Math.max(1, pick.rows) };
    else subActive = sub?.items.findIndex(enabled) ?? -1;
  }

  function choose(cols: number, rows: number) {
    const picker = sub?.picker;
    if (!picker || cols < 1 || rows < 1) return;
    closeMenu();
    picker(cols, rows);
  }

  function closeSub() {
    clearTimeout(closeTimer);
    sub = null;
    level = 0;
  }

  function hoverTop(i: number, item: Entry) {
    active = item.disabled ? -1 : i;
    level = 0;
    if (item.submenu || item.sizePicker) {
      void openSub(i);
    } else if (sub) {
      clearTimeout(closeTimer);
      closeTimer = setTimeout(closeSub, SUBMENU_GRACE);
    }
  }

  function run(item: MenuItem, index: number) {
    if (!enabled(item)) return;
    if (item.submenu || item.sizePicker) {
      void openSub(index, true);
      return;
    }
    closeMenu();
    item.action?.();
  }

  function step(items: MenuItem[], current: number, dir: 1 | -1): number {
    for (let i = 1; i <= items.length; i++) {
      const next = (current + dir * i + items.length * 2) % items.length;
      if (enabled(items[next])) return next;
    }
    return current;
  }

  function onKeydown(e: KeyboardEvent) {
    if (!open) return;
    const dismiss = () => {
      closeMenu();
      returnFocus?.focus({ preventScroll: true });
    };
    const inSub = level === 1 && sub;
    const clamp = (n: number) => Math.max(1, Math.min(GRID, n));
    const keys: Record<string, () => void> = inSub && sub?.picker
      ? {
          Escape: closeSub,
          ArrowLeft: () => (pick.cols <= 1 ? closeSub() : (pick = { ...pick, cols: clamp(pick.cols - 1) })),
          ArrowRight: () => (pick = { ...pick, cols: clamp(pick.cols + 1) }),
          ArrowUp: () => (pick = { ...pick, rows: clamp(pick.rows - 1) }),
          ArrowDown: () => (pick = { ...pick, rows: clamp(pick.rows + 1) }),
          Enter: () => choose(pick.cols, pick.rows),
          " ": () => choose(pick.cols, pick.rows),
          Tab: dismiss,
        }
      : inSub
      ? {
          Escape: closeSub,
          ArrowLeft: closeSub,
          ArrowDown: () => (subActive = step(sub!.items, subActive, 1)),
          ArrowUp: () => (subActive = step(sub!.items, subActive, -1)),
          Enter: () => subActive >= 0 && run(sub!.items[subActive], subActive),
          " ": () => subActive >= 0 && run(sub!.items[subActive], subActive),
          Tab: dismiss,
        }
      : {
          Escape: dismiss,
          ArrowDown: () => (active = step(open!.items, active, 1)),
          ArrowUp: () => (active = step(open!.items, active, -1)),
          ArrowRight: () => active >= 0 && void openSub(active, true),
          Enter: () => active >= 0 && run(open!.items[active], active),
          " ": () => active >= 0 && run(open!.items[active], active),
          Tab: dismiss,
        };
    const handler = keys[e.key];
    if (!handler) return;
    e.preventDefault();
    e.stopPropagation();
    handler();
  }

  function onPointerDown(e: PointerEvent) {
    if (!open) return;
    const target = e.target as Node;
    // The anchoring button toggles the menu itself on click.
    if (el?.contains(target) || subEl?.contains(target) || open.anchor?.contains(target)) return;
    closeMenu();
  }

  const hasLeading = (items: MenuItem[]) => items.some((i) => i !== "separator" && (i.icon || i.checked !== undefined));
</script>

<svelte:window
  onpointerdowncapture={onPointerDown}
  onkeydowncapture={onKeydown}
  onblur={closeMenu}
  onresize={closeMenu}
  onwheelcapture={(e) => open && !el?.contains(e.target as Node) && !subEl?.contains(e.target as Node) && closeMenu()}
/>

{#snippet entries(items: MenuItem[], current: number, nested: boolean)}
  {@const leading = hasLeading(items)}
  {#each items as item, i (i)}
    {#if item === "separator"}
      <div class="separator" role="separator"></div>
    {:else}
      <button
        class="item"
        class:active={current === i || (!nested && sub?.index === i)}
        class:destructive={item.destructive}
        data-index={i}
        role={item.checked !== undefined ? "menuitemcheckbox" : "menuitem"}
        aria-checked={item.checked}
        aria-haspopup={item.submenu || item.sizePicker ? "menu" : undefined}
        aria-expanded={item.submenu || item.sizePicker ? sub?.index === i : undefined}
        disabled={item.disabled}
        onpointerenter={() => {
          if (nested) {
            clearTimeout(closeTimer);
            level = 1;
            subActive = item.disabled ? -1 : i;
          } else hoverTop(i, item);
        }}
        onclick={() => run(item, i)}
      >
        {#if leading}
          <span class="lead">
            {#if item.checked}
              <Icon name="check" size={14} stroke={2.2} />
            {:else if item.icon}
              <Icon name={item.icon} size={16} />
            {/if}
          </span>
        {/if}
        <span class="label">{item.label}</span>
        {#if item.submenu || item.sizePicker}
          <span class="more"><Icon name="chevron-right" size={11} stroke={2.2} /></span>
        {:else if item.shortcut}
          <span class="shortcut">{item.shortcut}</span>
        {/if}
      </button>
    {/if}
  {/each}
{/snippet}

{#if open}
  <div
    bind:this={el}
    class="menu"
    role="menu"
    tabindex="-1"
    style:left="{pos.left}px"
    style:top="{pos.top}px"
    oncontextmenu={(e) => e.preventDefault()}
    onpointerleave={() => {
      if (!sub) active = -1;
    }}
  >
    {@render entries(open.items, level === 0 ? active : -1, false)}
  </div>
  {#if sub}
    <div
      bind:this={subEl}
      class="menu sub"
      role="menu"
      tabindex="-1"
      style:left="{sub.left}px"
      style:top="{sub.top}px"
      oncontextmenu={(e) => e.preventDefault()}
      onpointerenter={() => clearTimeout(closeTimer)}
      onpointerleave={() => (subActive = -1)}
    >
      {#if sub.picker}
        <div class="picker" role="group" aria-label="Table size" onpointerleave={() => level === 0 && (pick = { cols: 0, rows: 0 })}>
          {#each { length: GRID } as _, r (r)}
            {#each { length: GRID } as _, c (c)}
              <button
                class="square"
                class:on={c < pick.cols && r < pick.rows}
                aria-label="{c + 1} columns by {r + 1} rows"
                onpointerenter={() => (pick = { cols: c + 1, rows: r + 1 })}
                onclick={() => choose(c + 1, r + 1)}
              ></button>
            {/each}
          {/each}
        </div>
        <div class="picker-label">
          {#if pick.cols}{pick.cols} {pick.cols === 1 ? "column" : "columns"} × {pick.rows} {pick.rows === 1 ? "row" : "rows"}{:else}Choose a size{/if}
        </div>
      {:else}
        {@render entries(sub.items, subActive, true)}
      {/if}
    </div>
  {/if}
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 200;
    min-width: 200px;
    max-width: 320px;
    max-height: calc(100vh - 16px);
    overflow-y: auto;
    padding: 5px;
    border-radius: 12px;
    background: var(--glass-strong);
    backdrop-filter: saturate(180%) blur(30px);
    -webkit-backdrop-filter: saturate(180%) blur(30px);
    box-shadow:
      0 0 0 0.5px var(--glass-edge),
      0 1px 0 0 var(--glass-border) inset,
      0 10px 32px rgba(0, 0, 0, 0.16),
      0 2px 6px rgba(0, 0, 0, 0.08);
    outline: none;
    font: 13px/1 var(--font);
    color: var(--label);
    user-select: none;
    -webkit-user-select: none;
    transform-origin: top left;
    animation: menu-in 130ms var(--ease-out);
  }
  @keyframes menu-in {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 10px 0 8px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: default;
  }
  .lead {
    display: grid;
    place-items: center;
    width: 18px;
    flex: none;
    color: var(--label-2);
  }
  .label {
    flex: 1;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .more {
    display: grid;
    place-items: center;
    margin-left: 12px;
    color: var(--label-3);
  }
  .item.active .more {
    color: rgba(255, 255, 255, 0.85);
  }
  .picker {
    display: grid;
    grid-template-columns: repeat(8, 18px);
    gap: 3px;
    padding: 6px 6px 2px;
  }
  .square {
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: var(--fill);
    box-shadow: inset 0 0 0 0.5px var(--separator);
    cursor: default;
    transition: background 60ms;
  }
  .square.on {
    background: color-mix(in srgb, var(--accent) 70%, transparent);
    box-shadow: inset 0 0 0 0.5px var(--accent);
  }
  .picker-label {
    padding: 6px 6px 4px;
    color: var(--label-2);
    font-size: 12px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .menu.sub {
    animation: submenu-in 110ms var(--ease-out);
  }
  @keyframes submenu-in {
    from {
      opacity: 0;
    }
  }
  .shortcut {
    margin-left: 16px;
    color: var(--label-3);
    font-variant-numeric: tabular-nums;
  }
  .item.destructive,
  .item.destructive .lead {
    color: var(--red);
  }
  .item.active {
    background: var(--accent);
    color: #fff;
  }
  .item.active .lead,
  .item.active .shortcut {
    color: rgba(255, 255, 255, 0.85);
  }
  .item.destructive.active {
    background: var(--red);
  }
  .item.destructive.active .lead {
    color: #fff;
  }
  .item:disabled {
    color: var(--label-3);
  }
  .item:disabled .lead {
    color: var(--label-4);
  }
  .separator {
    height: 1px;
    margin: 5px 10px;
    background: var(--separator);
  }
</style>
