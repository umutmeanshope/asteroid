import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap, type CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { bracketMatching, indentOnInput, syntaxTree } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import { EditorView, drawSelection, dropCursor, keymap, placeholder } from "@codemirror/view";
import type { NoteSummary, TagCount } from "../types";
import { displayName, isMac, parentOf } from "../util";
import { toggleTask, toggleWrap } from "./commands";
import { editorContextMenu } from "./contextMenu";
import { nextCell, prevCell } from "./table";
import { livePreview } from "./livePreview";
import { Hashtag, MathSyntax, WikiLink, parseWikiLink } from "./markdownExt";
import { editorHighlighting, editorTheme } from "./theme";

export interface EditorOptions {
  doc: string;
  livePreview: boolean;
  spellcheck: boolean;
  getNotes: () => NoteSummary[];
  getTags: () => TagCount[];
  onChange: (doc: string) => void;
  /** `target` is a wikilink target (may include `#heading`) or a URL. */
  onOpenLink: (target: string, opts: { newTab: boolean; external: boolean }) => void;
  resolveImage: (target: string) => Promise<string | null>;
  renderEmbed: (target: string) => Promise<string | null>;
}

export interface EditorHandle {
  view: EditorView;
  setDoc(text: string): void;
  configure(opts: { livePreview: boolean; spellcheck: boolean }): void;
  revealLine(line: number): void;
  focus(): void;
  destroy(): void;
}

function linkCompletions(getNotes: () => NoteSummary[]) {
  return (ctx: CompletionContext): CompletionResult | null => {
    const match = ctx.matchBefore(/\[\[[^[\]|#\n]*$/);
    if (!match) return null;
    const notes = getNotes();
    const counts = new Map<string, number>();
    for (const n of notes) {
      const key = displayName(n.path).toLowerCase();
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return {
      from: match.from + 2,
      validFor: /^[^[\]|#\n]*$/,
      options: notes.map((n) => {
        const name = displayName(n.path);
        const unique = counts.get(name.toLowerCase()) === 1;
        const insert = unique ? name : n.kind === "note" ? n.path.replace(/\.md$/i, "") : n.path;
        return {
          label: name,
          detail: parentOf(n.path) || undefined,
          boost: n.kind === "note" ? 1 : 0,
          apply: (view: EditorView, _c: unknown, from: number, to: number) => {
            const closed = view.state.sliceDoc(to, to + 2) === "]]";
            view.dispatch({
              changes: { from, to, insert: closed ? insert : `${insert}]]` },
              selection: { anchor: from + insert.length + 2 },
            });
          },
        };
      }),
    };
  };
}

function tagCompletions(getTags: () => TagCount[]) {
  return (ctx: CompletionContext): CompletionResult | null => {
    const match = ctx.matchBefore(/(?:^|[\s(])#[\p{L}\p{N}_\-/]+$/u);
    if (!match) return null;
    const hash = match.text.indexOf("#");
    return {
      from: match.from + hash + 1,
      validFor: /^[\p{L}\p{N}_\-/]*$/u,
      options: getTags().map((t) => ({ label: t.tag, detail: String(t.count) })),
    };
  };
}

/** Finds the link under `pos` in the source (used for Cmd/Ctrl-click). */
function linkAt(state: EditorState, pos: number): { target: string; external: boolean } | null {
  for (let node: ReturnType<typeof syntaxTree>["topNode"] | null = syntaxTree(state).resolveInner(pos, 1); node; node = node.parent) {
    if (node.name === "WikiLink") {
      const link = parseWikiLink(state.sliceDoc(node.from, node.to));
      return { target: link.heading ? `${link.target}#${link.heading}` : link.target, external: false };
    }
    if (node.name === "Link" || node.name === "Image" || node.name === "Autolink") {
      const url = node.getChild("URL");
      if (!url) return null;
      const href = state.sliceDoc(url.from, url.to);
      return { target: href, external: /^[a-z][a-z0-9+.-]*:/i.test(href) };
    }
    if (node.name === "URL") {
      const href = state.sliceDoc(node.from, node.to);
      return { target: href, external: /^[a-z][a-z0-9+.-]*:/i.test(href) };
    }
  }
  return null;
}

export function createEditor(parent: HTMLElement, opts: EditorOptions): EditorHandle {
  const preview = new Compartment();
  const attrs = new Compartment();
  // Suppresses onChange while the document is replaced from disk.
  let applyingExternal = false;

  const previewExt = (on: boolean) =>
    livePreview({
      hideMarkup: on,
      resolveImage: opts.resolveImage,
      renderEmbed: opts.renderEmbed,
      openLink: (target) => opts.onOpenLink(target, { newTab: false, external: false }),
    });
  const attrsExt = (spell: boolean): Extension =>
    EditorView.contentAttributes.of({
      spellcheck: String(spell),
      autocorrect: spell ? "on" : "off",
      autocapitalize: "sentences",
    });

  const mod = (e: MouseEvent) => (isMac ? e.metaKey : e.ctrlKey);

  const state = EditorState.create({
    doc: opts.doc,
    extensions: [
      history(),
      drawSelection(),
      dropCursor(),
      indentOnInput(),
      bracketMatching(),
      closeBrackets(),
      highlightSelectionMatches(),
      search({ top: true }),
      EditorView.lineWrapping,
      EditorState.tabSize.of(4),
      placeholder("Start writing…"),
      markdown({ base: markdownLanguage, codeLanguages: languages, extensions: [WikiLink, Hashtag, MathSyntax] }),
      autocompletion({
        override: [linkCompletions(opts.getNotes), tagCompletions(opts.getTags)],
        icons: false,
      }),
      keymap.of([
        { key: "Mod-b", run: toggleWrap("**") },
        { key: "Mod-i", run: toggleWrap("*") },
        { key: "Mod-Shift-x", run: toggleWrap("~~") },
        { key: "Mod-Shift-c", run: toggleWrap("`") },
        { key: "Mod-k", run: toggleWrap("[[", "]]") },
        { key: "Mod-Enter", run: toggleTask },
        // In tables, Tab moves between cells (before indentWithTab).
        { key: "Tab", run: nextCell, shift: prevCell },
        ...closeBracketsKeymap,
        ...completionKeymap,
        ...searchKeymap,
        ...historyKeymap,
        indentWithTab,
        ...defaultKeymap,
      ]),
      editorTheme,
      editorHighlighting,
      preview.of(previewExt(opts.livePreview)),
      attrs.of(attrsExt(opts.spellcheck)),
      EditorView.updateListener.of((u) => {
        if (u.docChanged && !applyingExternal) opts.onChange(u.state.doc.toString());
      }),
      EditorView.domEventHandlers({
        contextmenu(e, view) {
          // Rendered links carry their target on the element; raw ones are parsed.
          const el = (e.target as HTMLElement).closest<HTMLElement>("[data-lp-target], [data-lp-href]");
          const href = el?.dataset.lpHref;
          const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
          const link = el
            ? { target: el.dataset.lpTarget ?? href ?? "", external: !!href && /^[a-z][a-z0-9+.-]*:/i.test(href) }
            : pos == null ? null : linkAt(view.state, pos);
          return editorContextMenu(e, view, link, (l, newTab) => opts.onOpenLink(l.target, { newTab, external: l.external }), opts.livePreview);
        },
        mousedown(e, view) {
          if (e.button !== 0) return false;
          const el = (e.target as HTMLElement).closest<HTMLElement>("[data-lp-target], [data-lp-href]");
          // Rendered links open on click; raw links need Cmd/Ctrl.
          if (el && (opts.livePreview || mod(e))) {
            const href = el.dataset.lpHref;
            const target = el.dataset.lpTarget ?? href ?? "";
            const lineActive = view.hasFocus && view.state.selection.ranges.some((r) => {
              const pos = view.posAtDOM(el);
              const line = view.state.doc.lineAt(pos);
              return r.from <= line.to && r.to >= line.from;
            });
            if (!lineActive || mod(e)) {
              e.preventDefault();
              opts.onOpenLink(target, { newTab: mod(e), external: !!href && /^[a-z][a-z0-9+.-]*:/i.test(href) });
              return true;
            }
          }
          if (mod(e)) {
            const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
            const link = pos == null ? null : linkAt(view.state, pos);
            if (link) {
              e.preventDefault();
              opts.onOpenLink(link.target, { newTab: e.shiftKey, external: link.external });
              return true;
            }
          }
          return false;
        },
      }),
    ],
  });

  const view = new EditorView({ state, parent });

  return {
    view,
    setDoc(text: string) {
      if (text === view.state.doc.toString()) return;
      const head = Math.min(view.state.selection.main.head, text.length);
      applyingExternal = true;
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: text },
        selection: { anchor: head },
        // External reloads should not be undone into.
        annotations: [],
      });
      applyingExternal = false;
    },
    configure({ livePreview: lp, spellcheck }) {
      opts.livePreview = lp;
      view.dispatch({ effects: [preview.reconfigure(previewExt(lp)), attrs.reconfigure(attrsExt(spellcheck))] });
    },
    revealLine(line: number) {
      const n = Math.max(1, Math.min(view.state.doc.lines, line + 1));
      const pos = view.state.doc.line(n).from;
      view.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: "center" }) });
      view.focus();
    },
    focus() {
      view.focus();
    },
    destroy() {
      view.destroy();
    },
  };
}
