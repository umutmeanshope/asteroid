<script lang="ts">
  import { tick } from "svelte";
  import { closeMenu, menu, type MenuItem } from "../lib/menu.svelte";
  import Icon from "./Icon.svelte";

  const MARGIN = 8;

  let el: HTMLDivElement | undefined = $state();
  let pos = $state({ left: 0, top: 0 });
  let active = $state(-1);
  /** Focused element before the menu opened, restored when it's dismissed by key. */
  let returnFocus: HTMLElement | null = null;
  let open = $derived(menu.current);

  type Action = Exclude<MenuItem, "separator">;
  const enabled = (item: MenuItem): item is Action => item !== "separator" && !item.disabled;

  // Place at the requested point, then keep the whole menu on screen: shift
  // left at the right edge, flip upwards at the bottom edge.
  $effect(() => {
    const m = open;
    if (!m) return;
    active = -1;
    returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    pos = { left: m.x, top: m.y };
    void tick().then(() => {
      if (!el || menu.current !== m) return;
      const { width, height } = el.getBoundingClientRect();
      let left = Math.min(m.x, innerWidth - MARGIN - width);
      let top = m.y;
      if (top + height > innerHeight - MARGIN) top = m.flipY - height;
      pos = { left: Math.max(MARGIN, left), top: Math.max(MARGIN, top) };
      el.focus({ preventScroll: true });
    });
  });

  function run(item: MenuItem) {
    if (!enabled(item)) return;
    closeMenu();
    item.action();
  }

  function step(dir: 1 | -1) {
    const items = open?.items ?? [];
    for (let i = 1; i <= items.length; i++) {
      const next = (active + dir * i + items.length * 2) % items.length;
      if (enabled(items[next])) {
        active = next;
        return;
      }
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (!open) return;
    const dismiss = () => {
      closeMenu();
      returnFocus?.focus({ preventScroll: true });
    };
    const keys: Record<string, () => void> = {
      Escape: dismiss,
      ArrowDown: () => step(1),
      ArrowUp: () => step(-1),
      Enter: () => active >= 0 && run(open!.items[active]),
      " ": () => active >= 0 && run(open!.items[active]),
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
    if (el?.contains(target) || open.anchor?.contains(target)) return;
    closeMenu();
  }
</script>

<svelte:window
  onpointerdowncapture={onPointerDown}
  onkeydowncapture={onKeydown}
  onblur={closeMenu}
  onresize={closeMenu}
  onwheelcapture={(e) => open && !el?.contains(e.target as Node) && closeMenu()}
/>

{#if open}
  {@const leading = open.items.some((i) => i !== "separator" && (i.icon || i.checked !== undefined))}
  <div
    bind:this={el}
    class="menu"
    role="menu"
    tabindex="-1"
    style:left="{pos.left}px"
    style:top="{pos.top}px"
    oncontextmenu={(e) => e.preventDefault()}
    onpointerleave={() => (active = -1)}
  >
    {#each open.items as item, i (i)}
      {#if item === "separator"}
        <div class="separator" role="separator"></div>
      {:else}
        <button
          class="item"
          class:active={active === i}
          class:destructive={item.destructive}
          role={item.checked !== undefined ? "menuitemcheckbox" : "menuitem"}
          aria-checked={item.checked}
          disabled={item.disabled}
          onpointerenter={() => (active = item.disabled ? -1 : i)}
          onclick={() => run(item)}
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
          {#if item.shortcut}<span class="shortcut">{item.shortcut}</span>{/if}
        </button>
      {/if}
    {/each}
  </div>
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
