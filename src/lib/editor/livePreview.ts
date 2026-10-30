// Live preview: renders Markdown in place while keeping the source editable.
// Syntax markers are hidden on lines that do not contain the cursor, and
// task markers, bullets, rules, image and note embeds and math become
// widgets. Structural styling (heading sizes, code blocks, quotes) is applied
// in both modes.
import { syntaxTree } from "@codemirror/language";
import { type EditorState, type Extension, type Range, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate, WidgetType } from "@codemirror/view";
import type { SyntaxNodeRef } from "@lezer/common";
import { renderMathIn, renderTex } from "../math";
import { mathSource, parseWikiLink } from "./markdownExt";

export interface LivePreviewConfig {
  /** Hide markup and render widgets (false = styled source mode). */
  hideMarkup: boolean;
  resolveImage: (target: string) => Promise<string | null>;
  /** HTML for an embedded note (`Note`, `Note#Heading`), or null if it isn't one. */
  renderEmbed: (target: string) => Promise<string | null>;
  openLink: (target: string) => void;
}

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|svg|bmp|avif|ico)$/i;

class BulletWidget extends WidgetType {
  eq() {
    return true;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = "cm-lp-bullet";
    span.textContent = "•";
    return span;
  }
}

class CheckboxWidget extends WidgetType {
  constructor(
    readonly checked: boolean,
    readonly pos: number,
  ) {
    super();
  }
  eq(other: CheckboxWidget) {
    return other.checked === this.checked && other.pos === this.pos;
  }
  toDOM(view: EditorView) {
    const box = document.createElement("span");
    box.className = `cm-lp-checkbox${this.checked ? " checked" : ""}`;
    box.setAttribute("role", "checkbox");
    box.setAttribute("aria-checked", String(this.checked));
    box.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
      view.dispatch({ changes: { from: this.pos + 1, to: this.pos + 2, insert: this.checked ? " " : "x" } });
    });
    return box;
  }
  ignoreEvent() {
    return true;
  }
}

class RuleWidget extends WidgetType {
  eq() {
    return true;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = "cm-lp-hr";
    return span;
  }
}

class LabelWidget extends WidgetType {
  constructor(
    readonly text: string,
    readonly className: string,
  ) {
    super();
  }
  eq(other: LabelWidget) {
    return other.text === this.text && other.className === this.className;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = this.className;
    span.textContent = this.text;
    return span;
  }
}

class ImageWidget extends WidgetType {
  constructor(
    readonly src: string,
    readonly alt: string,
    readonly width: number | undefined,
    readonly resolve: (target: string) => Promise<string | null>,
  ) {
    super();
  }
  eq(other: ImageWidget) {
    return other.src === this.src && other.width === this.width && other.alt === this.alt;
  }
  toDOM() {
    const wrap = document.createElement("span");
    wrap.className = "cm-lp-image";
    const img = document.createElement("img");
    img.alt = this.alt;
    if (this.width) img.style.width = `${this.width}px`;
    wrap.appendChild(img);
    if (/^https?:\/\//.test(this.src)) {
      img.src = this.src;
    } else {
      this.resolve(this.src).then((url) => {
        if (url) img.src = url;
        else {
          wrap.classList.add("missing");
          wrap.textContent = this.alt || this.src;
        }
      });
    }
    return wrap;
  }
}

class MathWidget extends WidgetType {
  constructor(
    readonly tex: string,
    readonly display: boolean,
    readonly block = false,
  ) {
    super();
  }
  eq(other: MathWidget) {
    return other.tex === this.tex && other.display === this.display && other.block === this.block;
  }
  toDOM(view: EditorView) {
    const el = document.createElement(this.block ? "div" : "span");
    el.className = this.block ? "cm-lp-math-block" : "cm-lp-math";
    renderTex(el, this.tex, this.display, () => view.requestMeasure());
    return el;
  }
}

/** `![[Note]]`: the embedded note, rendered like the reading view. Clicking
 *  its title opens it; clicking the body puts the cursor back on the embed. */
class EmbedWidget extends WidgetType {
  constructor(
    readonly target: string,
    readonly config: LivePreviewConfig,
  ) {
    super();
  }
  eq(other: EmbedWidget) {
    return other.target === this.target;
  }
  toDOM(view: EditorView) {
    const wrap = document.createElement("div");
    wrap.className = "cm-lp-embed-note";
    const body = document.createElement("div");
    body.className = "markdown embedded";
    wrap.appendChild(body);
    this.config.renderEmbed(this.target).then((html) => {
      if (html === null) {
        wrap.classList.add("missing");
        wrap.textContent = this.target;
        return;
      }
      body.innerHTML = html;
      body.querySelectorAll<HTMLImageElement>("img[data-vault-src]").forEach((img) => {
        this.config.resolveImage(img.dataset.vaultSrc!).then((url) => url && (img.src = url));
      });
      renderMathIn(body);
      view.requestMeasure();
    });
    wrap.addEventListener("click", (e) => {
      const link = (e.target as HTMLElement).closest<HTMLAnchorElement>("a.internal-link");
      if (!link) return;
      e.preventDefault();
      const heading = link.dataset.heading;
      this.config.openLink(heading ? `${link.dataset.href}#${heading}` : (link.dataset.href ?? ""));
    });
    return wrap;
  }
  // Links inside are handled above; elsewhere the editor moves the cursor.
  ignoreEvent(e: Event) {
    return !!(e.target as HTMLElement | null)?.closest?.("a");
  }
}

const bullet = Decoration.replace({ widget: new BulletWidget() });
const rule = Decoration.replace({ widget: new RuleWidget() });
const hidden = Decoration.replace({});

function safeDecode(s: string): string {
  try {
    return decodeURI(s);
  } catch {
    return s;
  }
}

function frontmatterEnd(state: EditorState): number {
  const doc = state.doc;
  if (doc.lines < 2 || doc.line(1).text !== "---") return -1;
  for (let i = 2; i <= Math.min(doc.lines, 400); i++) {
    if (doc.line(i).text.trimEnd() === "---") return doc.line(i).to;
  }
  return -1;
}

function build(view: EditorView, config: LivePreviewConfig): DecorationSet {
  const { state } = view;
  const doc = state.doc;
  const out: Range<Decoration>[] = [];
  const hide = config.hideMarkup;

  // Lines containing a selection show their raw Markdown.
  const activeLines = new Set<number>();
  if (view.hasFocus) {
    for (const r of state.selection.ranges) {
      const a = doc.lineAt(r.from).number;
      const b = doc.lineAt(r.to).number;
      for (let i = a; i <= b; i++) activeLines.add(i);
    }
  }
  const active = (from: number, to: number) => {
    const a = doc.lineAt(from).number;
    const b = doc.lineAt(to).number;
    for (let i = a; i <= b; i++) if (activeLines.has(i)) return true;
    return false;
  };
  const touchesSelection = (from: number, to: number) =>
    view.hasFocus && state.selection.ranges.some((r) => r.from <= to && r.to >= from);
  const lineClass = (pos: number, cls: string) => out.push(Decoration.line({ class: cls }).range(doc.lineAt(pos).from));
  const linesClass = (from: number, to: number, cls: string, edges = false) => {
    const first = doc.lineAt(from).number;
    const last = doc.lineAt(to).number;
    for (let i = first; i <= last; i++) {
      let c = cls;
      if (edges && i === first) c += ` ${cls}-first`;
      if (edges && i === last) c += ` ${cls}-last`;
      out.push(Decoration.line({ class: c }).range(doc.line(i).from));
    }
  };
  // View plugins may not replace line breaks, so replacements stay on one line.
  const sameLine = (from: number, to: number) => doc.lineAt(from).number === doc.lineAt(to).number;
  const replace = (deco: Decoration, from: number, to: number) => {
    if (to > from && sameLine(from, to)) out.push(deco.range(from, to));
  };
  const conceal = (from: number, to: number) => {
    if (hide) replace(hidden, from, to);
  };

  const fmEnd = frontmatterEnd(state);
  if (fmEnd > 0) linesClass(0, fmEnd, "cm-lp-frontmatter", true);

  for (const { from, to } of view.visibleRanges) {
    syntaxTree(state).iterate({
      from,
      to,
      enter: (node: SyntaxNodeRef) => {
        if (fmEnd > 0 && node.from <= fmEnd) {
          if (node.to <= fmEnd) return false;
          if (node.name !== "Document") return;
        }
        const name = node.name;

        const heading = /^ATXHeading(\d)$/.exec(name);
        if (heading) {
          lineClass(node.from, `cm-lp-h cm-lp-h${heading[1]}`);
          return;
        }

        switch (name) {
          case "HeaderMark": {
            if (node.node.parent?.name.startsWith("ATXHeading") && !active(node.from, node.to)) {
              const end = doc.sliceString(node.to, node.to + 1) === " " ? node.to + 1 : node.to;
              const start = node.from > doc.lineAt(node.from).from && doc.sliceString(node.from - 1, node.from) === " " ? node.from - 1 : node.from;
              conceal(start, end);
            }
            return;
          }
          case "EmphasisMark":
          case "StrikethroughMark": {
            const parent = node.node.parent;
            if (parent && !active(parent.from, parent.to)) conceal(node.from, node.to);
            return;
          }
          case "InlineCode": {
            out.push(Decoration.mark({ class: "cm-lp-inline-code" }).range(node.from, node.to));
            if (!active(node.from, node.to)) {
              for (const mark of node.node.getChildren("CodeMark")) conceal(mark.from, mark.to);
            }
            return false;
          }
          case "FencedCode":
          case "CodeBlock": {
            linesClass(node.from, node.to, "cm-lp-code", true);
            return false;
          }
          case "Blockquote": {
            const first = doc.lineAt(node.from).text;
            const callout = /^>\s*\[!(\w+)\]([+-]?)\s*(.*)$/.exec(first);
            if (callout) {
              const type = callout[1].toLowerCase();
              linesClass(node.from, node.to, `cm-lp-callout cm-lp-callout-${type}`, true);
              const line = doc.lineAt(node.from);
              const markerStart = line.from + first.indexOf("[!");
              const markerEnd = line.from + first.indexOf("]", first.indexOf("[!")) + 1 + callout[2].length;
              if (hide && !active(line.from, line.to)) {
                if (callout[3].trim()) {
                  conceal(markerStart, Math.min(markerEnd + 1, line.to));
                } else {
                  const label = type.charAt(0).toUpperCase() + type.slice(1);
                  replace(Decoration.replace({ widget: new LabelWidget(label, "cm-lp-callout-label") }), markerStart, markerEnd);
                }
              }
            } else {
              linesClass(node.from, node.to, "cm-lp-quote");
            }
            return;
          }
          case "QuoteMark": {
            if (!active(node.from, node.to)) {
              const end = doc.sliceString(node.to, node.to + 1) === " " ? node.to + 1 : node.to;
              conceal(node.from, end);
            }
            return;
          }
          case "HorizontalRule": {
            if (hide && !active(node.from, node.to)) replace(rule, node.from, node.to);
            return;
          }
          case "ListMark": {
            const item = node.node.parent;
            const isBullet = item?.parent?.name === "BulletList";
            const isTask = item?.getChild("Task") != null;
            if (!hide || active(node.from, node.to)) return;
            if (isTask) {
              const end = doc.sliceString(node.to, node.to + 1) === " " ? node.to + 1 : node.to;
              conceal(node.from, end);
            } else if (isBullet) {
              replace(bullet, node.from, node.to);
            }
            return;
          }
          case "TaskMarker": {
            const checked = /x/i.test(doc.sliceString(node.from + 1, node.to - 1));
            const task = node.node.parent;
            if (checked && task && task.to > node.to) {
              out.push(Decoration.mark({ class: "cm-lp-task-done" }).range(node.to, task.to));
            }
            if (hide && !touchesSelection(node.from, node.to)) {
              replace(Decoration.replace({ widget: new CheckboxWidget(checked, node.from) }), node.from, node.to);
            }
            return;
          }
          case "Link": {
            const marks = node.node.getChildren("LinkMark");
            const url = node.node.getChild("URL");
            if (!url || marks.length < 2) return;
            const href = doc.sliceString(url.from, url.to);
            const labelFrom = marks[0].to;
            const labelTo = marks[1].from;
            if (labelTo > labelFrom) {
              out.push(Decoration.mark({ class: "cm-lp-link", attributes: { "data-lp-href": href } }).range(labelFrom, labelTo));
            }
            if (!active(node.from, node.to)) {
              conceal(marks[0].from, marks[0].to);
              conceal(marks[1].from, node.to);
            }
            return false;
          }
          case "Image": {
            const url = node.node.getChild("URL");
            if (!url || !hide || active(node.from, node.to)) return false;
            const src = doc.sliceString(url.from, url.to);
            const altMatch = /^!\[([^\]]*)\]/.exec(doc.sliceString(node.from, node.to));
            const alt = altMatch?.[1] ?? "";
            const width = /^\d+$/.test(alt) ? Number(alt) : undefined;
            replace(Decoration.replace({ widget: new ImageWidget(safeDecode(src), alt, width, config.resolveImage) }), node.from, node.to);
            return false;
          }
          case "WikiLink": {
            const raw = doc.sliceString(node.from, node.to);
            const link = parseWikiLink(raw);
            const isActive = active(node.from, node.to);
            const width = link.alias && /^\d+$/.test(link.alias) ? Number(link.alias) : undefined;
            if (link.embed && IMAGE_EXT.test(link.target) && hide && !isActive) {
              replace(Decoration.replace({ widget: new ImageWidget(link.target, link.alias ?? "", width, config.resolveImage) }), node.from, node.to);
              return false;
            }
            // Note embeds: no extension, or .md (PDFs and others stay links).
            if (link.embed && hide && !isActive && !/\.(?!md$)[^./]+$/i.test(link.target)) {
              const target = link.heading ? `${link.target}#${link.heading}` : link.target;
              replace(Decoration.replace({ widget: new EmbedWidget(target, config) }), node.from, node.to);
              return false;
            }
            const open = link.embed ? 3 : 2;
            const target = link.heading ? `${link.target}#${link.heading}` : link.target;
            const pipe = raw.indexOf("|");
            const displayFrom = !isActive && pipe >= 0 ? node.from + pipe + 1 : node.from + open;
            const displayTo = node.to - 2;
            if (displayTo > displayFrom) {
              out.push(
                Decoration.mark({
                  class: link.embed ? "cm-lp-wikilink cm-lp-embed" : "cm-lp-wikilink",
                  attributes: { "data-lp-target": target },
                }).range(displayFrom, displayTo),
              );
            }
            if (!isActive) {
              conceal(node.from, displayFrom);
              conceal(node.to - 2, node.to);
            }
            return false;
          }
          case "Table": {
            linesClass(node.from, node.to, "cm-lp-table");
            return false;
          }
          case "InlineMath": {
            if (hide && !active(node.from, node.to)) {
              const { tex, display } = mathSource(doc.sliceString(node.from, node.to));
              replace(Decoration.replace({ widget: new MathWidget(tex, display) }), node.from, node.to);
            } else {
              out.push(Decoration.mark({ class: "cm-lp-math-src" }).range(node.from, node.to));
            }
            return false;
          }
          case "BlockMath": {
            // Rendered by the blockMath field when not being edited.
            linesClass(node.from, node.to, "cm-lp-math-src-block");
            return false;
          }
        }
      },
    });
  }
  return Decoration.set(out, true);
}

/**
 * `$$` math blocks rendered in place of their source lines. Replacing whole
 * lines needs a state field (view plugins may not hide line breaks), so the
 * cursor test uses the selection alone: a block shows its source while the
 * selection touches it.
 */
function blockMath(config: LivePreviewConfig): Extension {
  const compute = (state: EditorState): DecorationSet => {
    if (!config.hideMarkup) return Decoration.none;
    const out: Range<Decoration>[] = [];
    syntaxTree(state).iterate({
      enter: (node) => {
        if (node.name !== "BlockMath") return;
        const from = state.doc.lineAt(node.from).from;
        const to = state.doc.lineAt(node.to).to;
        if (!state.selection.ranges.some((r) => r.from <= to && r.to >= from)) {
          const { tex } = mathSource(state.doc.sliceString(node.from, node.to));
          out.push(Decoration.replace({ widget: new MathWidget(tex, true, true), block: true }).range(from, to));
        }
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

export function livePreview(config: LivePreviewConfig): Extension {
  return [blockMath(config), plugin(config)];
}

function plugin(config: LivePreviewConfig) {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = build(view, config);
      }
      update(u: ViewUpdate) {
        if (u.docChanged || u.viewportChanged || u.selectionSet || u.focusChanged || syntaxTree(u.startState) !== syntaxTree(u.state)) {
          this.decorations = build(u.view, config);
        }
      }
    },
    { decorations: (v) => v.decorations },
  );
}
