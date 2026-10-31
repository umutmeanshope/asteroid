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
import {
  type Align,
  type TableAt,
  addColumn,
  addRow,
  deleteColumn,
  deleteRow,
  deleteTable,
  formatTable,
  insertTable,
  setAlign,
  tableAt,
} from "./table";

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
  livePreview: boolean,
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
    ...tableItems(view, livePreview),
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

/** The table actions for cell (at.row, at.col). `widget`: invoked from the
 *  live-preview grid, which keeps focus in its cells. */
function tableActions(view: EditorView, at: TableAt, widget: boolean): MenuItem[] {
  const align = at.table.align[at.col];
  const aligned = (label: string, value: Align): MenuItem => ({
    label,
    checked: align === value,
    action: () => setAlign(view, at, value, widget),
  });
  const isHeader = at.row === 0;
  return [
    { label: "Add Row Above", icon: "row-above", disabled: isHeader, action: () => addRow(view, at, false, widget) },
    { label: "Add Row Below", icon: "row-below", action: () => addRow(view, at, true, widget) },
    { label: "Add Column Left", icon: "col-left", action: () => addColumn(view, at, false, widget) },
    { label: "Add Column Right", icon: "col-right", action: () => addColumn(view, at, true, widget) },
    "separator",
    aligned("Align Left", "left"),
    aligned("Align Center", "center"),
    aligned("Align Right", "right"),
    "separator",
    widget
      ? {
          label: "Edit as Markdown",
          icon: "code",
          action: () => {
            view.dispatch({ selection: { anchor: at.table.from }, scrollIntoView: true });
            view.focus();
          },
        }
      : { label: "Format Table", icon: "sparkle", action: () => formatTable(view, at) },
    "separator",
    { label: "Delete Row", destructive: true, disabled: isHeader, action: () => deleteRow(view, at, widget) },
    { label: "Delete Column", destructive: true, disabled: at.table.align.length <= 1, action: () => deleteColumn(view, at, widget) },
    { label: "Delete Table", icon: "trash", destructive: true, action: () => deleteTable(view, at) },
  ];
}

/** Right-click on a cell of the live-preview grid. */
export function tableMenu(e: MouseEvent, view: EditorView, at: TableAt, widget: boolean) {
  showMenu(e, tableActions(view, at, widget));
}

/** In a table (source view): "Table ›". Elsewhere: "Insert Table ›" with a size grid. */
function tableItems(view: EditorView, livePreview: boolean): MenuItem[] {
  const at = tableAt(view.state, view.state.selection.main.head);
  if (!at) {
    return [{ label: "Insert Table", icon: "table", sizePicker: (cols, rows) => insertTable(view, cols, rows, livePreview) }];
  }
  return [{ label: "Table", icon: "table", submenu: tableActions(view, at, false) }];
}
