// Lezer markdown extensions for Obsidian-flavoured syntax: [[wikilinks]],
// ![[embeds]], #tags and $math$.
import type { BlockContext, InlineContext, Line, MarkdownConfig } from "@lezer/markdown";
import { Tag, tags as t } from "@lezer/highlight";

export const hashtagTag = Tag.define();
export const mathTag = Tag.define();
export const wikiLinkTag = Tag.define(t.link);

const OPEN_BRACKET = 91; // [
const CLOSE_BRACKET = 93; // ]
const BANG = 33; // !
const HASH = 35; // #
const NEWLINE = 10;
const DOLLAR = 36;
const BACKSLASH = 92;

export const WikiLink: MarkdownConfig = {
  defineNodes: [
    { name: "WikiLink", style: wikiLinkTag },
    { name: "WikiLinkMark", style: t.processingInstruction },
  ],
  parseInline: [
    {
      name: "WikiLink",
      // Run before the standard Link/Image parsers so `[[` and `![[` are ours.
      before: "Link",
      parse(cx: InlineContext, next: number, pos: number) {
        let open: number;
        if (next === BANG) {
          if (cx.char(pos + 1) !== OPEN_BRACKET || cx.char(pos + 2) !== OPEN_BRACKET) return -1;
          open = pos + 3;
        } else if (next === OPEN_BRACKET) {
          if (cx.char(pos + 1) !== OPEN_BRACKET) return -1;
          open = pos + 2;
        } else {
          return -1;
        }
        for (let i = open; i < cx.end - 1; i++) {
          const c = cx.char(i);
          if (c === NEWLINE || c === OPEN_BRACKET) return -1;
          if (c === CLOSE_BRACKET && cx.char(i + 1) === CLOSE_BRACKET) {
            if (i === open) return -1;
            return cx.addElement(
              cx.elt("WikiLink", pos, i + 2, [cx.elt("WikiLinkMark", pos, open), cx.elt("WikiLinkMark", i, i + 2)]),
            );
          }
        }
        return -1;
      },
    },
  ],
};

const TAG_CHAR = /[\p{L}\p{N}_\-/]/u;
const DIGIT = /[0-9]/;

export const Hashtag: MarkdownConfig = {
  defineNodes: [{ name: "Hashtag", style: hashtagTag }],
  parseInline: [
    {
      name: "Hashtag",
      before: "Emphasis",
      parse(cx: InlineContext, next: number, pos: number) {
        if (next !== HASH) return -1;
        if (pos > cx.offset) {
          const prev = String.fromCharCode(cx.char(pos - 1));
          if (!/[\s(]/.test(prev)) return -1;
        }
        let end = pos + 1;
        let hasNonDigit = false;
        while (end < cx.end) {
          const ch = String.fromCharCode(cx.char(end));
          if (!TAG_CHAR.test(ch)) break;
          if (!DIGIT.test(ch)) hasNonDigit = true;
          end++;
        }
        if (end === pos + 1 || !hasNonDigit) return -1;
        return cx.addElement(cx.elt("Hashtag", pos, end));
      },
    },
  ],
};

const isSpace = (c: number) => c === 32 || c === 9 || c === NEWLINE;
const isDigit = (c: number) => c >= 48 && c <= 57;

/**
 * `$inline$` and `$$display$$` math, following Pandoc's rules so prices don't
 * become formulas: the opening `$` isn't followed by a space, the closing one
 * isn't preceded by a space or followed by a digit ("$5 and $6" stays text).
 * Inline math stays on one line; `$$` on its own line starts a block.
 */
export const MathSyntax: MarkdownConfig = {
  defineNodes: [
    { name: "InlineMath", style: mathTag },
    { name: "BlockMath", block: true, style: mathTag },
    { name: "MathMark", style: t.processingInstruction },
  ],
  parseInline: [
    {
      name: "InlineMath",
      before: "Emphasis",
      parse(cx: InlineContext, next: number, pos: number) {
        if (next !== DOLLAR) return -1;
        const display = cx.char(pos + 1) === DOLLAR;
        const start = pos + (display ? 2 : 1);
        const first = cx.char(start);
        if (first === -1 || first === DOLLAR || (!display && isSpace(first))) return -1;
        for (let i = start; i < cx.end; i++) {
          const c = cx.char(i);
          if (c === NEWLINE) return -1;
          if (c === BACKSLASH) {
            i++;
            continue;
          }
          if (c !== DOLLAR) continue;
          if (display) {
            if (cx.char(i + 1) !== DOLLAR) return -1;
            return cx.addElement(cx.elt("InlineMath", pos, i + 2, [cx.elt("MathMark", pos, start), cx.elt("MathMark", i, i + 2)]));
          }
          if (isSpace(cx.char(i - 1)) || isDigit(cx.char(i + 1))) return -1;
          return cx.addElement(cx.elt("InlineMath", pos, i + 1, [cx.elt("MathMark", pos, start), cx.elt("MathMark", i, i + 1)]));
        }
        return -1;
      },
    },
  ],
  parseBlock: [
    {
      name: "BlockMath",
      before: "FencedCode",
      parse(cx: BlockContext, line: Line) {
        const text = line.text.slice(line.pos);
        if (!text.startsWith("$$")) return false;
        const rest = text.slice(2).trimEnd();
        const from = cx.lineStart + line.pos;
        // "$$x$$" alone on a line is a one-line block; "$$x$$ and more" is inline.
        if (rest.endsWith("$$") && rest.length > 2) {
          if (rest.slice(0, -2).includes("$$")) return false;
          cx.addElement(cx.elt("BlockMath", from, cx.lineStart + line.text.length));
          cx.nextLine();
          return true;
        }
        if (rest.includes("$$")) return false;
        let to = cx.lineStart + line.text.length;
        while (cx.nextLine()) {
          to = cx.lineStart + line.text.length;
          if (line.text.trimEnd().endsWith("$$")) {
            cx.nextLine();
            break;
          }
        }
        cx.addElement(cx.elt("BlockMath", from, to));
        return true;
      },
    },
  ],
};

/** The TeX inside a math node's source, without its `$` delimiters. */
export function mathSource(raw: string): { tex: string; display: boolean } {
  const s = raw.trim();
  const display = s.startsWith("$$");
  const n = display ? 2 : 1;
  return { tex: s.slice(n, s.endsWith(display ? "$$" : "$") && s.length >= 2 * n ? -n : undefined).trim(), display };
}

/** Splits the inside of a wikilink into target, heading and display text. */
export function parseWikiLink(raw: string): { embed: boolean; target: string; heading?: string; alias?: string } {
  const embed = raw.startsWith("!");
  const inner = raw.slice(embed ? 3 : 2, -2);
  const pipe = inner.indexOf("|");
  const ref = pipe >= 0 ? inner.slice(0, pipe) : inner;
  const alias = pipe >= 0 ? inner.slice(pipe + 1) : undefined;
  const hash = ref.indexOf("#");
  return {
    embed,
    target: (hash >= 0 ? ref.slice(0, hash) : ref).trim(),
    heading: hash >= 0 ? ref.slice(hash + 1).trim() : undefined,
    alias,
  };
}
