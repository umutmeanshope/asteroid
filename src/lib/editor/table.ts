// Markdown table editing: find the table around the cursor, edit it as rows
// and cells, and write it back with aligned columns. Used by the grid widget
// in live preview, the context menu, and Tab / Shift-Tab in source mode.
//
// Operations run in one of two modes. In source mode they move the editor's
// cursor into the Markdown. In widget mode (live preview's grid) they leave
// the cursor alone, since a cursor inside the table would show its source,
// and instead ask the redrawn grid to focus a cell (focusRequests).
import { syntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

export type Align = "left" | "center" | "right" | null;

export interface Table {
  /** Document range of the table's lines (start of first to end of last). */
  from: number;
  to: number;
  /** rows[0] is the header; the delimiter row isn't included. */
  rows: string[][];
  align: Align[];
}

export interface TableAt {
  table: Table;
  /** Cell under the position; row 0 is the header. */
  row: number;
  col: number;
}

const DELIMITER = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/;

/** Positions of the `|` separators in a row, skipping `\|` and code spans. */
function pipes(line: string): number[] {
  const out: number[] = [];
  let code = 0;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (ch === "\\") i++;
    else if (ch === "`") {
      let run = 1;
      while (line[i + run] === "`") run++;
      code = code === run ? 0 : code || run;
      i += run - 1;
    } else if (ch === "|" && !code) out.push(i);
  }
  return out;
}

/** The trimmed cell texts of a row, without the outer pipes. */
function cells(line: string): string[] {
  const bars = pipes(line);
  const bounds = [-1, ...bars, line.length];
  const parts: string[] = [];
  for (let i = 0; i < bounds.length - 1; i++) parts.push(line.slice(bounds[i] + 1, bounds[i + 1]));
  const trimmed = line.trim();
  if (trimmed.startsWith("|")) parts.shift();
  if (trimmed.endsWith("|") && !trimmed.endsWith("\\|") && parts.length) parts.pop();
  return parts.map((c) => c.trim());
}

function parseAlign(cell: string): Align {
  const left = cell.startsWith(":");
  const right = cell.endsWith(":");
  return left && right ? "center" : right ? "right" : left ? "left" : null;
}

const inCode = (state: EditorState, pos: number) => {
  for (let node: ReturnType<typeof syntaxTree>["topNode"] | null = syntaxTree(state).resolveInner(pos, 1); node; node = node.parent) {
    if (node.name === "FencedCode" || node.name === "CodeBlock") return true;
  }
  return false;
};

/** The table containing `pos`, or null. */
export function tableAt(state: EditorState, pos: number): TableAt | null {
  const doc = state.doc;
  const line = doc.lineAt(pos);
  const isRow = (n: number) => n >= 1 && n <= doc.lines && doc.line(n).text.includes("|") && doc.line(n).text.trim() !== "";
  if (!isRow(line.number) || inCode(state, pos)) return null;
  let first = line.number;
  let last = line.number;
  while (isRow(first - 1)) first--;
  while (isRow(last + 1)) last++;
  // The delimiter row must be the block's second line.
  if (last - first < 1 || !DELIMITER.test(doc.line(first + 1).text)) return null;

  const header = cells(doc.line(first).text);
  const align = cells(doc.line(first + 1).text).map(parseAlign);
  const body: string[][] = [];
  for (let n = first + 2; n <= last; n++) body.push(cells(doc.line(n).text));
  const cols = Math.max(header.length, align.length, ...body.map((r) => r.length), 1);
  const pad = (r: string[]) => Array.from({ length: cols }, (_, i) => r[i] ?? "");
  const table: Table = {
    from: doc.line(first).from,
    to: doc.line(last).to,
    rows: [pad(header), ...body.map(pad)],
    align: Array.from({ length: cols }, (_, i) => align[i] ?? null),
  };

  const offset = pos - line.from;
  const bars = pipes(line.text);
  const leading = line.text.trim().startsWith("|") ? 1 : 0;
  const col = Math.min(cols - 1, Math.max(0, bars.filter((b) => b < offset).length - leading));
  const index = line.number - first;
  return { table, row: index <= 1 ? 0 : index - 1, col };
}

const width = (s: string) => [...s].length;

/** The table as Markdown with aligned columns, and where each cell's text starts. */
export function render(t: Table): { text: string; cellStart: number[][]; cellEnd: number[][] } {
  const cols = t.align.length;
  const widths = Array.from({ length: cols }, (_, c) => Math.max(3, ...t.rows.map((r) => width(r[c] ?? ""))));
  const pad = (s: string, c: number) => {
    const gap = widths[c] - width(s);
    if (t.align[c] === "right") return " ".repeat(gap) + s;
    if (t.align[c] === "center") return " ".repeat(Math.floor(gap / 2)) + s + " ".repeat(Math.ceil(gap / 2));
    return s + " ".repeat(gap);
  };
  const delimiter = widths.map((w, c) => {
    const a = t.align[c];
    if (a === "center") return `:${"-".repeat(w - 2)}:`;
    if (a === "right") return `${"-".repeat(w - 1)}:`;
    if (a === "left") return `:${"-".repeat(w - 1)}`;
    return "-".repeat(w);
  });
  const lines: string[] = [];
  const cellStart: number[][] = [];
  const cellEnd: number[][] = [];
  let offset = 0;
  const addRow = (row: string[], record: boolean) => {
    let line = "|";
    const starts: number[] = [];
    const ends: number[] = [];
    row.forEach((cell, c) => {
      const text = record ? pad(cell, c) : cell;
      const lead = record ? text.length - text.trimStart().length : 0;
      starts.push(offset + line.length + 1 + lead);
      ends.push(offset + line.length + 1 + lead + cell.length);
      line += ` ${text} |`;
    });
    if (record) {
      cellStart.push(starts);
      cellEnd.push(ends);
    }
    lines.push(line);
    offset += line.length + 1;
  };
  addRow(t.rows[0], true);
  addRow(delimiter, false);
  for (const row of t.rows.slice(1)) addRow(row, true);
  return { text: lines.join("\n"), cellStart, cellEnd };
}

/** A cell the grid widget should focus once the table at `from` is redrawn. */
export interface FocusRequest {
  from: number;
  row: number;
  col: number;
  select: boolean;
}

export const focusRequests = new WeakMap<EditorView, FocusRequest>();

/** Writes `t` over the table at `at` and puts the cursor in cell (row, col). */
function apply(view: EditorView, old: Table, t: Table, row: number, col: number, select = false, widget = false) {
  const { text, cellStart, cellEnd } = render(t);
  const r = Math.max(0, Math.min(row, t.rows.length - 1));
  const c = Math.max(0, Math.min(col, t.align.length - 1));
  if (widget) {
    focusRequests.set(view, { from: old.from, row: r, col: c, select });
    view.dispatch({ changes: { from: old.from, to: old.to, insert: text }, userEvent: "input" });
    return;
  }
  const anchor = old.from + cellStart[r][c];
  const head = select ? old.from + cellEnd[r][c] : anchor;
  view.dispatch({
    changes: { from: old.from, to: old.to, insert: text },
    selection: { anchor, head },
    scrollIntoView: true,
    userEvent: "input",
  });
  view.focus();
}

const clone = (t: Table): Table => ({ ...t, rows: t.rows.map((r) => [...r]), align: [...t.align] });

/**
 * Inserts a `cols` × `rows` table (rows include the header) on its own lines
 * at the cursor. In source mode the first header cell is selected; in widget
 * mode the cursor goes below the table and the grid focuses that cell.
 */
export function insertTable(view: EditorView, cols = 3, rows = 3, widget = false) {
  const { state } = view;
  const pos = state.selection.main.head;
  const line = state.doc.lineAt(pos);
  const table: Table = {
    from: 0,
    to: 0,
    rows: [Array.from({ length: cols }, (_, i) => `Column ${i + 1}`), ...Array.from({ length: Math.max(0, rows - 1) }, () => Array(cols).fill(""))],
    align: Array(cols).fill(null),
  };
  const { text, cellStart, cellEnd } = render(table);
  const blankLine = line.text.trim() === "";
  // On its own lines, with a blank line before so it isn't read as paragraph text...
  const before = blankLine ? (line.number > 1 && state.doc.line(line.number - 1).text.trim() !== "" ? "\n" : "") : "\n\n";
  // ...and a line after, which a following paragraph needs to not join the table.
  const after = widget || line.number === state.doc.lines ? "\n" : "";
  const at = blankLine ? line.from : line.to;
  const start = at + before.length;
  if (widget) focusRequests.set(view, { from: start, row: 0, col: 0, select: true });
  view.dispatch({
    changes: { from: at, to: blankLine ? line.to : at, insert: before + text + after },
    selection: widget ? { anchor: start + text.length + 1 } : { anchor: start + cellStart[0][0], head: start + cellEnd[0][0] },
    scrollIntoView: true,
    userEvent: "input",
  });
  if (!widget) view.focus();
}

export function addRow(view: EditorView, at: TableAt, below: boolean, widget = false) {
  const t = clone(at.table);
  // Nothing goes above the header: "above" the header means the first body row.
  const index = below || at.row === 0 ? at.row + 1 : at.row;
  t.rows.splice(index, 0, Array(t.align.length).fill(""));
  apply(view, at.table, t, index, at.col, false, widget);
}

export function addColumn(view: EditorView, at: TableAt, right: boolean, widget = false) {
  const t = clone(at.table);
  const index = right ? at.col + 1 : at.col;
  for (const row of t.rows) row.splice(index, 0, "");
  t.align.splice(index, 0, null);
  apply(view, at.table, t, at.row, index, false, widget);
}

export function deleteRow(view: EditorView, at: TableAt, widget = false) {
  if (at.row === 0) return;
  const t = clone(at.table);
  t.rows.splice(at.row, 1);
  apply(view, at.table, t, Math.min(at.row, t.rows.length - 1), at.col, false, widget);
}

export function deleteColumn(view: EditorView, at: TableAt, widget = false) {
  if (at.table.align.length <= 1) return;
  const t = clone(at.table);
  for (const row of t.rows) row.splice(at.col, 1);
  t.align.splice(at.col, 1);
  apply(view, at.table, t, at.row, Math.min(at.col, t.align.length - 1), false, widget);
}

export function setAlign(view: EditorView, at: TableAt, align: Align, widget = false) {
  const t = clone(at.table);
  t.align[at.col] = align;
  apply(view, at.table, t, at.row, at.col, false, widget);
}

export function formatTable(view: EditorView, at: TableAt) {
  apply(view, at.table, at.table, at.row, at.col);
}

export function deleteTable(view: EditorView, at: TableAt) {
  const doc = view.state.doc;
  const to = at.table.to < doc.length ? at.table.to + 1 : at.table.to;
  view.dispatch({ changes: { from: at.table.from, to }, selection: { anchor: at.table.from }, userEvent: "delete" });
  view.focus();
}

/** Replaces one cell's Markdown, re-aligning the table (used while typing in the grid). */
export function setCell(view: EditorView, at: TableAt, row: number, col: number, markdown: string) {
  const t = clone(at.table);
  if (!t.rows[row] || col >= t.align.length) return;
  t.rows[row][col] = markdown;
  const { text } = render(t);
  if (text === view.state.sliceDoc(at.table.from, at.table.to)) return;
  view.dispatch({ changes: { from: at.table.from, to: at.table.to, insert: text }, userEvent: "input.type" });
}

/** Adds an empty row at the end (Tab or Enter past the last row in the grid). */
export function appendRow(view: EditorView, at: TableAt, col: number) {
  const t = clone(at.table);
  t.rows.push(Array(t.align.length).fill(""));
  apply(view, at.table, t, t.rows.length - 1, col, false, true);
}

/** Tab: next cell, adding a row after the last one. Selects the cell's text. */
export function nextCell(view: EditorView): boolean {
  const at = tableAt(view.state, view.state.selection.main.head);
  if (!at) return false;
  const t = clone(at.table);
  let { row, col } = at;
  if (++col >= t.align.length) {
    col = 0;
    if (++row >= t.rows.length) t.rows.push(Array(t.align.length).fill(""));
  }
  apply(view, at.table, t, row, col, true);
  return true;
}

/** Shift-Tab: previous cell. */
export function prevCell(view: EditorView): boolean {
  const at = tableAt(view.state, view.state.selection.main.head);
  if (!at) return false;
  let { row, col } = at;
  if (--col < 0) {
    if (row === 0) col = 0;
    else {
      row--;
      col = at.table.align.length - 1;
    }
  }
  apply(view, at.table, at.table, row, col, true);
  return true;
}
