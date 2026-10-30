// In-app context menus. Native GTK menus can't take the app's glass styling,
// so menus are drawn in HTML by <ContextMenu>, mounted once in App.svelte.

import type { IconName } from "../components/Icon.svelte";

export type MenuItem =
  | {
      label: string;
      action: () => void;
      icon?: IconName;
      /** Shows a checkmark in the leading slot instead of the icon. */
      checked?: boolean;
      destructive?: boolean;
      disabled?: boolean;
      shortcut?: string;
    }
  | "separator";

export interface OpenMenu {
  x: number;
  y: number;
  /** Top edge of the anchoring button, for flipping the menu above it. */
  flipY: number;
  items: MenuItem[];
  anchor: Element | null;
}

class MenuState {
  current = $state.raw<OpenMenu | null>(null);
}

export const menu = new MenuState();

/**
 * Opens a menu for a right-click (at the cursor) or a button click (beneath
 * the button, like a macOS pull-down). Clicking the same button again closes it.
 * Pass `button` when opening after an await, once `e.currentTarget` is gone.
 */
export function showMenu(e: MouseEvent, items: MenuItem[], button?: Element) {
  e.preventDefault();
  e.stopPropagation();
  const anchor = e.type === "contextmenu" ? null : (button ?? (e.currentTarget as Element | null));
  if (anchor && menu.current?.anchor === anchor) {
    menu.current = null;
    return;
  }
  if (anchor) {
    const r = anchor.getBoundingClientRect();
    menu.current = { x: r.left, y: r.bottom + 6, flipY: r.top - 6, items, anchor };
  } else {
    menu.current = { x: e.clientX, y: e.clientY, flipY: e.clientY, items, anchor: null };
  }
}

export function closeMenu() {
  menu.current = null;
}
