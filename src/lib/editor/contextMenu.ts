// The editor's right-click menu: link actions, clipboard and Markdown
// formatting, drawn by the app's own <ContextMenu>. Shift+right-click still
// opens the webview's native menu (spelling suggestions, Inspect in dev).

import { EditorSelection, type StateCommand } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import type { IconName } from "../../components/Icon.svelte";
import { copyText, pasteText } from "../clipboard";
import { type MenuItem, showMenu } from "../menu.svelte";
import { shortcutLabel as shortcut } from "../util";
import { toggleTask, toggleWrap } from "./commands";

export interface LinkHit {
  target: string;
  external: boolean;
}

function selectedText(view: EditorView): string {
  const { state } = view;
  return state.selection.ranges
    .filter((r) => !r.empty)
    .map((r) => state.sliceDoc(r.from, r.to))
    .join(state.lineBreak);
}

export function editorContextMenu(
  e: MouseEvent,
  view: EditorView,
  link: LinkHit | null,
  openLink: (link: LinkHit, newTab: boolean) => void,
): boolean {
  if (e.shiftKey) return false;

  // Like macOS: a right-click outside the selection moves the cursor there.
  const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
  if (pos != null && !view.state.selection.ranges.some((r) => r.from <= pos && pos <= r.to)) {
    view.dispatch({ selection: EditorSelection.cursor(pos) });
  }
  view.focus();

  const hasSelection = view.state.selection.ranges.some((r) => !r.empty);
  const run = (cmd: StateCommand) => () => {
    cmd({ state: view.state, dispatch: view.dispatch });
    view.focus();
  };
  const format = (label: string, icon: IconName, keys: string, cmd: StateCommand): MenuItem => ({
    label,
    icon,
    shortcut: shortcut(keys),
    action: run(cmd),
  });

  const items: MenuItem[] = [];
  if (link) {
    items.push({ label: link.external ? "Open Link" : "Open Note", icon: link.external ? "external" : "note", action: () => openLink(link, false) });
    if (!link.external) items.push({ label: "Open in New Tab", icon: "plus", action: () => openLink(link, true) });
    items.push({ label: "Copy Link", icon: "link", action: () => void copyText(link.target) }, "separator");
  }
  items.push(
    {
      label: "Cut",
      icon: "scissors",
      shortcut: shortcut("Mod-x"),
      disabled: !hasSelection,
      action: () => {
        void copyText(selectedText(view));
        view.dispatch(view.state.replaceSelection(""), { userEvent: "delete.cut" });
        view.focus();
      },
    },
    {
      label: "Copy",
      icon: "copy",
      shortcut: shortcut("Mod-c"),
      disabled: !hasSelection,
      action: () => void copyText(selectedText(view)),
    },
    {
      label: "Paste",
      icon: "clipboard",
      shortcut: shortcut("Mod-v"),
      action: () =>
        void pasteText().then((text) => {
          if (text) view.dispatch(view.state.replaceSelection(text), { userEvent: "input.paste", scrollIntoView: true });
          view.focus();
        }),
    },
    "separator",
    format("Bold", "bold", "Mod-b", toggleWrap("**")),
    format("Italic", "italic", "Mod-i", toggleWrap("*")),
    format("Strikethrough", "strikethrough", "Mod-Shift-x", toggleWrap("~~")),
    format("Inline Code", "code", "Mod-Shift-c", toggleWrap("`")),
    "separator",
    format("Insert Link", "link", "Mod-k", toggleWrap("[[", "]]")),
    format("Toggle Checklist", "checklist", "Mod-Enter", toggleTask),
    "separator",
    {
      label: "Select All",
      icon: "select-all",
      shortcut: shortcut("Mod-a"),
      action: () => {
        view.dispatch({ selection: EditorSelection.single(0, view.state.doc.length) });
        view.focus();
      },
    },
  );
  showMenu(e, items);
  return true;
}
