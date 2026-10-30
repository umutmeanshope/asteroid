// KaTeX math rendering, loaded on first use so notes without math don't pay
// for it. Rendering goes through katex.render (DOM nodes, not an HTML string).
import type katexType from "katex";

type Katex = typeof katexType;

let katex: Katex | null = null;
let loading: Promise<Katex> | null = null;

export function loadKatex(): Promise<Katex> {
  loading ??= Promise.all([import("katex"), import("katex/dist/katex.min.css")]).then(([m]) => (katex = m.default));
  return loading;
}

/** Renders `tex` into `el`; synchronous once KaTeX has loaded. */
export function renderTex(el: HTMLElement, tex: string, display: boolean, onDone?: () => void) {
  const draw = (k: Katex) => {
    k.render(tex, el, { displayMode: display, throwOnError: false, output: "htmlAndMathml" });
    onDone?.();
  };
  if (katex) draw(katex);
  else
    void loadKatex()
      .then(draw)
      .catch(() => (el.textContent = tex));
}

/** Typesets the `.math` spans the Rust renderer emits for `$…$` and `$$…$$`. */
export function renderMathIn(root: HTMLElement) {
  const spans = root.querySelectorAll<HTMLElement>("span.math:not([data-rendered])");
  for (const span of spans) {
    span.dataset.rendered = "";
    renderTex(span, span.textContent ?? "", span.classList.contains("math-display"));
  }
}
