// A PDF's table of contents, with every destination resolved to a page and,
// when the PDF says, the vertical position of the heading on that page.
import type { PDFDocumentProxy } from "./pdf";

export interface OutlineItem {
  /** Position in the tree, e.g. "2.0.1"; stable for one document. */
  id: string;
  title: string;
  /** 1-based page, or null if the destination can't be resolved. */
  page: number | null;
  /** Top of the destination in PDF units (from the page bottom), if given. */
  top: number | null;
  /** External link entries open in the browser instead. */
  url: string | null;
  bold: boolean;
  italic: boolean;
  children: OutlineItem[];
}

type RawItem = Awaited<ReturnType<PDFDocumentProxy["getOutline"]>>[number];

export async function loadOutline(doc: PDFDocumentProxy): Promise<OutlineItem[]> {
  const raw = await doc.getOutline();
  if (!raw?.length) return [];
  const convert = (items: RawItem[], prefix: string): Promise<OutlineItem[]> =>
    Promise.all(
      items.map(async (item, i) => {
        const id = prefix ? `${prefix}.${i}` : String(i);
        const { page, top } = await resolve(doc, item.dest);
        return {
          id,
          title: item.title.trim() || "Untitled",
          page,
          top,
          url: item.url ?? null,
          bold: !!item.bold,
          italic: !!item.italic,
          children: await convert(item.items ?? [], id),
        };
      }),
    );
  return convert(raw, "");
}

/** Named or explicit destination → page and top. Malformed ones resolve to nothing. */
async function resolve(doc: PDFDocumentProxy, dest: RawItem["dest"]): Promise<{ page: number | null; top: number | null }> {
  try {
    const explicit = typeof dest === "string" ? await doc.getDestination(dest) : dest;
    if (!Array.isArray(explicit) || explicit.length === 0) return { page: null, top: null };
    const ref = explicit[0];
    const index = typeof ref === "number" ? ref : await doc.getPageIndex(ref);
    // [ref, {name: "XYZ"}, left, top, zoom] | [ref, {name: "FitH"}, top] | ...
    const mode = (explicit[1] as { name?: string } | undefined)?.name;
    const top = mode === "XYZ" ? explicit[3] : mode === "FitH" || mode === "FitBH" ? explicit[2] : mode === "FitR" ? explicit[5] : null;
    return { page: index + 1, top: typeof top === "number" ? top : null };
  } catch {
    return { page: null, top: null };
  }
}

/** Every item in document order, with its depth. */
export function flatten(items: OutlineItem[], depth = 0, out: { item: OutlineItem; depth: number }[] = []) {
  for (const item of items) {
    out.push({ item, depth });
    flatten(item.children, depth + 1, out);
  }
  return out;
}
