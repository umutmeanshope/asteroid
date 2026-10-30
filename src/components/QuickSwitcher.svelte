<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { displayName, fuzzyScore, modKey, parentOf } from "../lib/util";
  import Palette, { type PaletteItem } from "./Palette.svelte";
  import type { IconName } from "./Icon.svelte";

  let query = $state("");
  const icons: Record<string, IconName> = { note: "note", pdf: "pdf", image: "image", other: "file", folder: "folder" };

  // Recently opened tabs first when the query is empty.
  let items = $derived.by((): PaletteItem[] => {
    const q = query.trim();
    let ranked;
    if (!q) {
      const open = new Set(app.tabs.map((t) => t.path));
      ranked = [...app.notes].sort((a, b) => Number(open.has(b.path)) - Number(open.has(a.path))).slice(0, 60);
    } else {
      ranked = app.notes
        .map((n) => {
          const name = displayName(n.path);
          const score = Math.max(
            fuzzyScore(q, name) + 50,
            fuzzyScore(q, n.path),
            ...(n.aliases ?? []).map((a) => fuzzyScore(q, a) + 40),
          );
          return { n, score };
        })
        .filter((r) => r.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 60)
        .map((r) => r.n);
    }
    const out: PaletteItem[] = ranked.map((n) => ({
      id: n.path,
      title: displayName(n.path),
      subtitle: parentOf(n.path) || undefined,
      icon: icons[n.kind],
      iconClass: n.kind,
      run: ({ newTab }) => app.openFile(n.path, { newTab }),
    }));
    const exact = app.notes.some((n) => displayName(n.path).toLowerCase() === q.toLowerCase());
    if (q && !exact && !/[\\:]/.test(q)) {
      out.push({
        id: "__create",
        title: `Create “${q}”`,
        icon: "compose",
        iconClass: "accent",
        run: () => void app.newNote(parentOf(q), q.slice(q.lastIndexOf("/") + 1)),
      });
    }
    return out;
  });
</script>

<Palette
  placeholder="Open a note or file…"
  bind:query
  {items}
  footer="↵ open   {modKey} ↵ open in new tab   esc close"
/>
