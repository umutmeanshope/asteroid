// Live preview's table editor: Markdown tables shown as a real grid whose
// cells are edited in place, like a word processor. Every edit writes the
// whole table back as aligned Markdown, so the file stays plain text.
//
// The grid replaces the table's lines, which only a state field may do. The
// table shows its source when the editor's selection enters it (e.g. via
// "Edit as Markdown"). Cells are their own small contenteditable islands; the
// widget swallows their events (ignoreEvent) so CodeMirror leaves them alone.
import { syntaxTree } from "@codemirror/language";
import { type EditorState, type Extension, type Range, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, WidgetType } from "@codemirror/view";
import { tableMenu } from "./contextMenu";
import { type Align, type TableAt, appendRow, focusRequests, setCell, tableAt } from "./table";

export interface TableWidgetConfig {
  hideMarkup: boolean;
  openLink: (target: string) => void;
}

const escapeHtml = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
/** Cell Markdown as typed text: `\|` reads as `|`. */
const toText = (md: string) => md.replace(/\\\|/g, "|");
/** Typed text as cell Markdown: pipes escaped, no line breaks. */
const toMarkdown = (text: string) => text.replace(/\r?\n/g, " ").replace(/(^|[^\\])\|/g, "$1\\|");

/** A small inline renderer for cells that aren't being edited. */
function inlineHtml(md: string): string {
  return escapeHtml(toText(md))
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^*])\*([^*\s][^*]*)\*/g, "$1<em>$2</em>")
    .replace(/~~([^~]+)~~/g, "<del>$1</del>")
    .replace(/\[\[([^\]|]+)\|([^\]]+)\]\]/g, '<span class="link" data-target="$1">$2</span>')
    .replace(/\[\[([^\]]+)\]\]/g, '<span class="link" data-target="$1">$1</span>')
    .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, '<span class="link" data-target="$2">$1</span>');
}

const alignStyle = (a: Align) => a ?? "left";

class TableWidget extends WidgetType {
  constructor(
    readonly markdown: string,
    readonly rows: string[][],
    readonly align: Align[],
    readonly config: TableWidgetConfig,
  ) {
    super();
  }

  eq(other: TableWidget) {
    return other.markdown === this.markdown;
  }

  toDOM(view: EditorView) {
    const wrap = document.createElement("div");
    wrap.className = "cm-table-widget";
    wrap.contentEditable = "false";
    const table = document.createElement("table");
    wrap.appendChild(table);
    this.rows.forEach((row, r) => {
      const tr = document.createElement("tr");
      row.forEach((md, c) => {
        const td = document.createElement(r === 0 ? "th" : "td");
        const cell = document.createElement("div");
        cell.className = "cell";
        cell.contentEditable = "true";
        cell.spellcheck = false;
        cell.dataset.row = String(r);
        cell.dataset.col = String(c);
        cell.dataset.md = md;
        cell.innerHTML = inlineHtml(md);
        td.style.textAlign = alignStyle(this.align[c]);
        td.appendChild(cell);
        tr.appendChild(td);
      });
      table.appendChild(tr);
    });
    this.listen(wrap, view);
    this.focusRequested(wrap, view);
    return wrap;
  }

  /** Same shape: patch cell contents in place, never the cell being typed in. */
  updateDOM(dom: HTMLElement, view: EditorView) {
    const rows = dom.querySelectorAll("tr");
    if (rows.length !== this.rows.length || this.rows.some((r, i) => rows[i].children.length !== r.length)) return false;
    this.rows.forEach((row, r) =>
      row.forEach((md, c) => {
        const td = rows[r].children[c] as HTMLElement;
        const cell = td.firstElementChild as HTMLElement;
        td.style.textAlign = alignStyle(this.align[c]);
        if (cell.dataset.md === md) return;
        cell.dataset.md = md;
        if (cell !== document.activeElement) cell.innerHTML = inlineHtml(md);
      }),
    );
    this.focusRequested(dom, view);
    return true;
  }

  ignoreEvent() {
    return true;
  }

  /** Focuses the cell an operation asked for (see table.ts focusRequests). */
  private focusRequested(dom: HTMLElement, view: EditorView) {
    const request = focusRequests.get(view);
    if (!request) return;
    requestAnimationFrame(() => {
      if (focusRequests.get(view) !== request || !dom.isConnected) return;
      let from: number;
      try {
        from = view.posAtDOM(dom);
      } catch {
        return;
      }
      if (from !== request.from) return;
      focusRequests.delete(view);
      const cell = dom.querySelector<HTMLElement>(`.cell[data-row="${request.row}"][data-col="${request.col}"]`);
      if (cell) focusCell(cell, request.select ? "all" : "end");
    });
  }

  private listen(wrap: HTMLElement, view: EditorView) {
    const at = (): TableAt | null => {
      try {
        return tableAt(view.state, view.posAtDOM(wrap));
      } catch {
        return null;
      }
    };
    const cellOf = (e: Event) => (e.target as HTMLElement).closest<HTMLElement>(".cell");
    const coords = (cell: HTMLElement) => ({ row: Number(cell.dataset.row), col: Number(cell.dataset.col) });
    const cellAt = (row: number, col: number) => wrap.querySelector<HTMLElement>(`.cell[data-row="${row}"][data-col="${col}"]`);

    wrap.addEventListener("focusin", (e) => {
      const cell = cellOf(e);
      if (!cell) return;
      cell.classList.add("editing");
      // Show the raw text for editing. Plain cells already do, so leave them
      // alone and keep the caret where the click put it.
      const raw = toText(cell.dataset.md ?? "");
      if (cell.innerHTML !== escapeHtml(raw)) {
        cell.textContent = raw;
        focusCell(cell, "end");
      }
    });
    wrap.addEventListener("focusout", (e) => {
      const cell = cellOf(e);
      if (!cell) return;
      cell.classList.remove("editing");
      cell.innerHTML = inlineHtml(cell.dataset.md ?? "");
    });
    wrap.addEventListener("input", (e) => {
      const cell = cellOf(e);
      const table = at();
      if (!cell || !table) return;
      const md = toMarkdown(cell.textContent ?? "").trim();
      cell.dataset.md = md;
      const { row, col } = coords(cell);
      setCell(view, table, row, col, md);
    });
    // Plain text only, on one line.
    wrap.addEventListener("paste", (e) => {
      if (!cellOf(e)) return;
      e.preventDefault();
      const text = (e.clipboardData?.getData("text/plain") ?? "").replace(/\s*\r?\n\s*/g, " ");
      document.execCommand("insertText", false, text);
    });
    // Links in cells open with Cmd/Ctrl-click, like in the editor.
    wrap.addEventListener("mousedown", (e) => {
      const link = (e.target as HTMLElement).closest<HTMLElement>(".link");
      if (link && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        this.config.openLink(link.dataset.target ?? "");
      }
    });
    wrap.addEventListener("contextmenu", (e) => {
      const cell = cellOf(e);
      const table = at();
      if (!cell || !table) return;
      const { row, col } = coords(cell);
      tableMenu(e, view, { ...table, row, col }, true);
    });
    wrap.addEventListener("keydown", (e) => {
      const cell = cellOf(e);
      if (!cell) return;
      const { row, col } = coords(cell);
      const rows = wrap.querySelectorAll("tr").length;
      const cols = wrap.querySelector("tr")?.children.length ?? 1;
      const caret = caretOffset(cell);
      const length = (cell.textContent ?? "").length;
      const go = (r: number, c: number, mode: "all" | "start" | "end" = "all") => {
        const next = cellAt(r, c);
        if (next) focusCell(next, mode);
      };
      const leave = (after: boolean) => {
        const table = at();
        if (!table) return;
        const pos = after ? Math.min(view.state.doc.length, table.table.to + 1) : Math.max(0, table.table.from - 1);
        view.dispatch({ selection: { anchor: pos }, scrollIntoView: true });
        view.focus();
      };
      const extend = () => {
        const table = at();
        if (table) appendRow(view, table, e.key === "Tab" ? 0 : col);
      };
      let handled = true;
      switch (e.key) {
        case "Tab":
          if (e.shiftKey) {
            if (col > 0) go(row, col - 1);
            else if (row > 0) go(row - 1, cols - 1);
          } else if (col < cols - 1) go(row, col + 1);
          else if (row < rows - 1) go(row + 1, 0);
          else extend();
          break;
        case "Enter":
          if (row < rows - 1) go(row + 1, col);
          else extend();
          break;
        case "ArrowUp":
          if (row > 0) go(row - 1, col, "end");
          else leave(false);
          break;
        case "ArrowDown":
          if (row < rows - 1) go(row + 1, col, "end");
          else leave(true);
          break;
        case "ArrowLeft":
          if (caret === 0 && !hasSelection()) {
            if (col > 0) go(row, col - 1, "end");
            else if (row > 0) go(row - 1, cols - 1, "end");
            else handled = false;
          } else handled = false;
          break;
        case "ArrowRight":
          if (caret === length && !hasSelection()) {
            if (col < cols - 1) go(row, col + 1, "start");
            else if (row < rows - 1) go(row + 1, 0, "start");
            else handled = false;
          } else handled = false;
          break;
        case "Escape":
          leave(true);
          break;
        default:
          handled = false;
      }
      if (handled) {
        e.preventDefault();
        e.stopPropagation();
      }
    });
  }
}

function hasSelection(): boolean {
  const sel = window.getSelection();
  return !!sel && !sel.isCollapsed;
}

function caretOffset(cell: HTMLElement): number {
  const sel = window.getSelection();
  if (!sel || !sel.rangeCount || !cell.contains(sel.anchorNode)) return 0;
  const range = sel.getRangeAt(0).cloneRange();
  range.selectNodeContents(cell);
  range.setEnd(sel.anchorNode!, sel.anchorOffset);
  return range.toString().length;
}

function focusCell(cell: HTMLElement, mode: "all" | "start" | "end") {
  cell.focus({ preventScroll: false });
  const sel = window.getSelection();
  if (!sel) return;
  const range = document.createRange();
  range.selectNodeContents(cell);
  if (mode !== "all") range.collapse(mode === "start");
  sel.removeAllRanges();
  sel.addRange(range);
}

/** Grids for every table the selection isn't in. */
export function tableWidgets(config: TableWidgetConfig): Extension {
  const compute = (state: EditorState): DecorationSet => {
    if (!config.hideMarkup) return Decoration.none;
    const out: Range<Decoration>[] = [];
    syntaxTree(state).iterate({
      enter: (node) => {
        if (node.name !== "Table") return;
        const at = tableAt(state, node.from);
        if (!at) return false;
        const { from, to } = at.table;
        if (state.selection.ranges.some((r) => r.from <= to && r.to >= from)) return false;
        const markdown = state.sliceDoc(from, to);
        out.push(Decoration.replace({ widget: new TableWidget(markdown, at.table.rows, at.table.align, config), block: true }).range(from, to));
        return false;
      },
    });
    return Decoration.set(out, true);
  };
  return StateField.define<DecorationSet>({
    create: compute,
    update(value, tr) {
      return tr.docChanged || tr.selection || syntaxTree(tr.startState) !== syntaxTree(tr.state) ? compute(tr.state) : value;
    },
    provide: (field) => EditorView.decorations.from(field),
  });
}
