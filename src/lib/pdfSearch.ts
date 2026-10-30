// Find-in-PDF: per-page text extracted from pdf.js, matched case-insensitively,
// and painted into the page's text layer as highlight spans.
import type { PDFPageProxy } from "./pdf";

/** A page's searchable text and where each text-layer item starts in it. */
export interface PageText {
  text: string;
  /** starts[i] is the offset of text item i (= text-layer div i). */
  starts: number[];
}

export interface Match {
  page: number;
  start: number;
  end: number;
}

/**
 * Lower-cases character by character, keeping the length: plain toLowerCase
 * turns "İ" into two characters, which would shift every offset after it.
 */
export function foldCase(s: string): string {
  let out = "";
  for (const ch of s) {
    const lower = ch.toLowerCase();
    out += lower.length === ch.length ? lower : ch;
  }
  return out;
}

/**
 * The page's text items joined in text-layer order. Items ending a line are
 * followed by a space so words across line breaks still match; that space
 * belongs to no item. Marked-content entries are skipped, as the text layer
 * skips them.
 */
export async function extractPageText(page: PDFPageProxy): Promise<PageText> {
  const content = await page.getTextContent();
  let text = "";
  const starts: number[] = [];
  for (const item of content.items) {
    if (!("str" in item)) continue;
    starts.push(text.length);
    text += item.str;
    if (item.hasEOL && !text.endsWith(" ")) text += " ";
  }
  return { text: foldCase(text), starts };
}

export function findInPage(page: PageText, query: string, pageNumber: number): Match[] {
  const needle = foldCase(query.replace(/\s+/g, " ").trim());
  if (!needle) return [];
  const out: Match[] = [];
  for (let i = page.text.indexOf(needle); i >= 0; i = page.text.indexOf(needle, i + needle.length)) {
    out.push({ page: pageNumber, start: i, end: i + needle.length });
  }
  return out;
}

/** The text layer of one rendered page: its item divs and their texts. */
export interface Layer {
  divs: HTMLElement[];
  strs: string[];
}

/**
 * Rewrites the page's text-layer divs so `matches` show as highlights; the
 * one at index `selected` (within `matches`) gets the "selected" style.
 * Calling it again with no matches restores the plain text.
 */
export function paintHighlights(layer: Layer, page: PageText, matches: Match[], selected: number) {
  // Per item: the [from, to) character ranges to highlight and whether selected.
  const ranges = new Map<number, { from: number; to: number; selected: boolean }[]>();
  matches.forEach((m, index) => {
    for (let item = itemAt(page, m.start); item < page.starts.length && page.starts[item] < m.end; item++) {
      const itemStart = page.starts[item];
      const itemEnd = itemStart + (layer.strs[item]?.length ?? 0);
      const from = Math.max(m.start, itemStart) - itemStart;
      const to = Math.min(m.end, itemEnd) - itemStart;
      if (to <= from) continue;
      if (!ranges.has(item)) ranges.set(item, []);
      ranges.get(item)!.push({ from, to, selected: index === selected });
    }
  });
  layer.divs.forEach((div, i) => {
    const str = layer.strs[i] ?? "";
    const marks = ranges.get(i);
    if (!marks) {
      if (div.dataset.highlighted !== undefined) {
        div.textContent = str;
        delete div.dataset.highlighted;
      }
      return;
    }
    div.dataset.highlighted = "";
    div.replaceChildren();
    let pos = 0;
    for (const { from, to, selected: isSelected } of marks.sort((a, b) => a.from - b.from)) {
      if (from > pos) div.append(str.slice(pos, from));
      const span = document.createElement("span");
      span.className = isSelected ? "highlight selected" : "highlight";
      span.textContent = str.slice(Math.max(from, pos), to);
      div.append(span);
      pos = Math.max(pos, to);
    }
    if (pos < str.length) div.append(str.slice(pos));
  });
}

/** The item containing character `offset` (binary search over starts). */
function itemAt(page: PageText, offset: number): number {
  let lo = 0;
  let hi = page.starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (page.starts[mid] <= offset) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}
