<script lang="ts">
  import { onMount } from "svelte";
  import { tick } from "svelte";
  import { api, message } from "../lib/api";
  import type { PDFDocumentProxy, RenderTask } from "../lib/pdf";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { flatten, loadOutline, type OutlineItem } from "../lib/pdfOutline";
  import { extractPageText, findInPage, type Layer, type Match, type PageText, paintHighlights } from "../lib/pdfSearch";
  import { app } from "../lib/state.svelte";
  import { debounce, isMac } from "../lib/util";
  import Icon from "./Icon.svelte";

  let { path, active = true }: { path: string; active?: boolean } = $props();

  interface PageSize {
    width: number;
    height: number;
  }

  let scroller: HTMLDivElement | undefined = $state();
  let doc = $state.raw<PDFDocumentProxy | null>(null);
  let sizes = $state<PageSize[]>([]);
  let scale = $state(1);
  let fit = $state(true);
  let current = $state(1);
  /** How far into the current page the reading line is, in CSS pixels. */
  let currentOffset = $state(0);
  let error = $state<string | null>(null);
  let pageInput = $state("1");

  // pdf.js is large; load it only when a PDF is opened.
  let pdfModule: typeof import("../lib/pdf") | undefined;

  const rendered = new Map<number, { scale: number; task?: RenderTask }>();

  // ---------- table of contents ----------
  /** null while loading; [] when the PDF has none. */
  let outline = $state.raw<OutlineItem[] | null>(null);
  let collapsed = $state<Record<string, boolean>>({});
  let outlineList: HTMLDivElement | undefined = $state();
  let allItems = $derived(outline ? flatten(outline) : []);
  let rows = $derived.by(() => {
    const out: { item: OutlineItem; depth: number }[] = [];
    const walk = (items: OutlineItem[], depth: number) => {
      for (const item of items) {
        out.push({ item, depth });
        if (item.children.length && !collapsed[item.id]) walk(item.children, depth + 1);
      }
    };
    walk(outline ?? [], 0);
    return out;
  });
  /** The section being read: the last entry that starts above the reading
   *  line (a third of the way down the view), using its position on the page. */
  let currentId = $derived.by(() => {
    let id: string | null = null;
    for (const { item } of allItems) {
      if (item.page === null || item.page > current) continue;
      const height = sizes[item.page - 1]?.height ?? 0;
      const startsAt = item.top === null ? 0 : (height - item.top) * scale;
      if (item.page < current || startsAt <= currentOffset) id = item.id;
    }
    return id ?? allItems.find(({ item }) => item.page !== null)?.item.id ?? null;
  });

  async function initOutline(pdf: PDFDocumentProxy) {
    const items = await loadOutline(pdf).catch(() => []);
    // Long outlines start with only the top level open.
    if (flatten(items).length > 40) {
      const closed: Record<string, boolean> = {};
      for (const item of items) if (item.children.length) closed[item.id] = true;
      collapsed = closed;
    }
    outline = items;
  }

  async function openItem(item: OutlineItem) {
    if (item.url) {
      void openUrl(item.url).catch(() => {});
      return;
    }
    if (!doc || item.page === null || !scroller) return;
    const holder = scroller.querySelector<HTMLDivElement>(`[data-page="${item.page}"]`);
    if (!holder) return;
    let offset = 0;
    if (item.top !== null) {
      const viewport = (await doc.getPage(item.page)).getViewport({ scale });
      offset = Math.max(0, viewport.convertToViewportPoint(0, item.top)[1] - 12);
    }
    scroller.scrollTo({ top: holder.offsetTop + offset });
  }

  // Keep the current section visible in a long outline.
  $effect(() => {
    if (currentId === null || !app.pdfOutlineOpen) return;
    outlineList?.querySelector(`[data-id="${currentId}"]`)?.scrollIntoView({ block: "nearest" });
  });

  // ---------- find in PDF ----------
  let searchOpen = $state(false);
  let query = $state("");
  let matches = $state.raw<Match[]>([]);
  let selected = $state(-1);
  let searching = $state(false);
  /** Every page searched and none had any text: likely a scanned PDF. */
  let noText = $state(false);
  let searchInput: HTMLInputElement | undefined = $state();
  const pageTexts = new Map<number, PageText>();
  const layers = new Map<number, Layer>();
  let searchRun = 0;
  /** A match to scroll to once its page has rendered. */
  let pendingReveal = -1;

  async function pageText(n: number): Promise<PageText> {
    let text = pageTexts.get(n);
    if (!text) {
      text = await extractPageText(await doc!.getPage(n));
      pageTexts.set(n, text);
    }
    return text;
  }

  /** Repaints one rendered page's highlights for the current matches. */
  function paintPage(n: number) {
    const layer = layers.get(n);
    const text = pageTexts.get(n);
    if (!layer || !text) return;
    const onPage = matches.filter((m) => m.page === n);
    const current = matches[selected];
    paintHighlights(layer, text, onPage, current?.page === n ? onPage.indexOf(current) : -1);
  }

  function paintAll() {
    for (const n of layers.keys()) paintPage(n);
  }

  async function runSearch(q: string) {
    const run = ++searchRun;
    matches = [];
    selected = -1;
    noText = false;
    paintAll();
    if (!doc || !q.trim()) {
      searching = false;
      return;
    }
    searching = true;
    let anyText = false;
    // Pages in order, so matches stay sorted; the first match at or after the
    // page being read is selected as soon as it turns up.
    for (let n = 1; n <= doc.numPages; n++) {
      const text = await pageText(n);
      if (run !== searchRun) return;
      anyText ||= text.text.trim().length > 0;
      const found = findInPage(text, q, n);
      if (found.length) {
        matches = [...matches, ...found];
        if (selected === -1 && n >= current) reveal(matches.length - found.length);
        else paintPage(n);
      }
    }
    searching = false;
    noText = !anyText;
    if (selected === -1 && matches.length) reveal(0);
  }

  const searchSoon = debounce((q: string) => void runSearch(q), 180);

  /** Selects match `i`, highlights it and scrolls it into view. */
  function reveal(i: number) {
    const previous = matches[selected];
    selected = i;
    const match = matches[i];
    if (previous && previous.page !== match?.page) paintPage(previous.page);
    if (!match) return;
    paintPage(match.page);
    if (layers.has(match.page)) scrollToSelected();
    else {
      pendingReveal = i;
      goTo(match.page);
    }
  }

  function scrollToSelected() {
    scroller?.querySelector(".textLayer .highlight.selected")?.scrollIntoView({ block: "center", inline: "nearest" });
  }

  function step(dir: 1 | -1) {
    if (!matches.length) return;
    reveal((selected + dir + matches.length) % matches.length);
  }

  async function openSearch() {
    searchOpen = true;
    await tick();
    searchInput?.focus();
    searchInput?.select();
  }

  function closeSearch() {
    searchOpen = false;
    searchRun++;
    searching = false;
    matches = [];
    selected = -1;
    paintAll();
  }

  function onKeydown(e: KeyboardEvent) {
    if (!active || !doc) return;
    const mod = isMac ? e.metaKey : e.ctrlKey;
    const key = e.key.toLowerCase();
    if (mod && key === "f" && !e.shiftKey) {
      e.preventDefault();
      void openSearch();
    } else if (mod && key === "g" && searchOpen) {
      e.preventDefault();
      step(e.shiftKey ? -1 : 1);
    } else if (key === "escape" && searchOpen) {
      e.preventDefault();
      closeSearch();
    }
  }
  let observer: IntersectionObserver | undefined;
  const visible = new Set<number>();

  function fitScale() {
    if (!scroller || !sizes.length) return 1;
    const widest = Math.max(...sizes.map((s) => s.width));
    return Math.max(0.25, Math.min(4, (scroller.clientWidth - 64) / widest));
  }

  async function renderPage(n: number) {
    if (!doc || !scroller) return;
    const holder = scroller.querySelector<HTMLDivElement>(`[data-page="${n}"]`);
    if (!holder) return;
    const done = rendered.get(n);
    if (done && done.scale === scale) return;
    done?.task?.cancel();

    const target = scale;
    const entry: { scale: number; task?: RenderTask } = { scale: target };
    rendered.set(n, entry);
    try {
      const page = await doc.getPage(n);
      const viewport = page.getViewport({ scale: target });
      const ratio = window.devicePixelRatio || 1;
      const canvas = document.createElement("canvas");
      canvas.width = Math.floor(viewport.width * ratio);
      canvas.height = Math.floor(viewport.height * ratio);
      canvas.style.width = `${viewport.width}px`;
      canvas.style.height = `${viewport.height}px`;
      const task = page.render({
        canvas,
        viewport,
        transform: ratio !== 1 ? [ratio, 0, 0, ratio, 0, 0] : undefined,
      });
      entry.task = task;
      await task.promise;
      if (rendered.get(n) !== entry) return;

      const textLayer = document.createElement("div");
      textLayer.className = "textLayer";
      holder.replaceChildren(canvas, textLayer);
      holder.style.setProperty("--scale-factor", String(target));
      holder.style.setProperty("--total-scale-factor", String(target));
      holder.style.setProperty("--user-unit", "1");
      const layer = new pdfModule!.pdfjs.TextLayer({ textContentSource: page.streamTextContent(), container: textLayer, viewport });
      await layer.render();
      if (rendered.get(n) !== entry) return;
      layers.set(n, { divs: layer.textDivs, strs: layer.textContentItemsStr });
      if (matches.some((m) => m.page === n)) {
        await pageText(n);
        paintPage(n);
        if (pendingReveal >= 0 && matches[pendingReveal]?.page === n) {
          pendingReveal = -1;
          scrollToSelected();
        }
      }
    } catch (e) {
      if ((e as { name?: string })?.name !== "RenderingCancelledException") rendered.delete(n);
    }
  }

  function relayout() {
    for (const n of visible) void renderPage(n);
  }

  function setScale(next: number, keepFit = false) {
    const clamped = Math.max(0.25, Math.min(5, next));
    if (!scroller) return;
    const anchor = scroller.scrollTop / Math.max(1, scroller.scrollHeight);
    scale = clamped;
    fit = keepFit;
    requestAnimationFrame(() => {
      if (scroller) scroller.scrollTop = anchor * scroller.scrollHeight;
      relayout();
    });
  }

  function onScroll() {
    if (!scroller) return;
    const mid = scroller.scrollTop + scroller.clientHeight / 3;
    let y = 16;
    for (let i = 0; i < sizes.length; i++) {
      const top = y;
      y += sizes[i].height * scale + 16;
      if (y > mid) {
        current = i + 1;
        currentOffset = mid - top;
        pageInput = String(current);
        return;
      }
    }
  }

  function goTo(n: number) {
    const target = scroller?.querySelector<HTMLDivElement>(`[data-page="${n}"]`);
    target?.scrollIntoView({ block: "start" });
  }

  function onWheel(e: WheelEvent) {
    if (!e.ctrlKey && !e.metaKey) return;
    e.preventDefault();
    setScale(scale * (e.deltaY < 0 ? 1.1 : 1 / 1.1));
  }

  onMount(() => {
    let cancelled = false;
    let resize: ResizeObserver | undefined;

    (async () => {
      try {
        const [data, mod] = await Promise.all([api.readFile(path), import("../lib/pdf")]);
        pdfModule = mod;
        const pdf = await mod.loadPdf(data);
        if (cancelled) {
          void pdf.loadingTask.destroy();
          return;
        }
        // Page sizes up front so scrolling and the page counter are stable.
        const list: PageSize[] = await Promise.all(
          Array.from({ length: pdf.numPages }, async (_, i) => {
            const vp = (await pdf.getPage(i + 1)).getViewport({ scale: 1 });
            return { width: vp.width, height: vp.height };
          }),
        );
        doc = pdf;
        sizes = list;
        scale = fitScale();
        void initOutline(pdf);

        observer = new IntersectionObserver(
          (entries) => {
            for (const entry of entries) {
              const n = Number((entry.target as HTMLElement).dataset.page);
              if (entry.isIntersecting) {
                visible.add(n);
                void renderPage(n);
              } else {
                visible.delete(n);
              }
            }
          },
          { root: scroller, rootMargin: "800px 0px" },
        );
        requestAnimationFrame(() => scroller?.querySelectorAll("[data-page]").forEach((el) => observer!.observe(el)));

        resize = new ResizeObserver(() => {
          if (fit) setScale(fitScale(), true);
        });
        resize.observe(scroller!);
      } catch (e) {
        error = message(e);
      }
    })();

    return () => {
      cancelled = true;
      observer?.disconnect();
      resize?.disconnect();
      for (const r of rendered.values()) r.task?.cancel();
      void doc?.loadingTask.destroy();
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div class="pdf" class:with-outline={app.pdfOutlineOpen && !!doc}>
  {#if error}
    <div class="state"><Icon name="warning" size={30} stroke={1.4} /><p>Could not open PDF: {error}</p></div>
  {:else if !doc}
    <div class="state"><span class="spinner"></span></div>
  {/if}

  <div class="scroller" bind:this={scroller} onscroll={onScroll} onwheel={onWheel}>
    {#each sizes as size, i}
      <div
        class="page"
        data-page={i + 1}
        style:width="{size.width * scale}px"
        style:height="{size.height * scale}px"
      ></div>
    {/each}
  </div>

  {#if doc && app.pdfOutlineOpen}
    <aside class="outline glass" aria-label="Table of contents">
      <header>
        <h3>Contents</h3>
        <button
          class="icon-btn"
          title="Hide"
          onclick={() => {
            app.pdfOutlineOpen = false;
            app.saveWorkspace();
          }}><Icon name="xmark" size={13} stroke={2} /></button
        >
      </header>
      <div class="list" bind:this={outlineList} role="tree">
        {#if outline === null}
          <div class="empty"><span class="spinner"></span></div>
        {:else if !outline.length}
          <div class="empty">
            <Icon name="outline" size={26} stroke={1.4} />
            <p>This PDF has no table of contents.</p>
          </div>
        {:else}
          {#each rows as { item, depth } (item.id)}
            <div
              class="entry"
              class:current={item.id === currentId}
              class:bold={item.bold}
              class:italic={item.italic}
              class:dead={item.page === null && !item.url}
              style:padding-left="{6 + depth * 14}px"
              data-id={item.id}
              role="treeitem"
              aria-selected={item.id === currentId}
              aria-expanded={item.children.length ? !collapsed[item.id] : undefined}
            >
              {#if item.children.length}
                <button class="chevron" class:open={!collapsed[item.id]} aria-label={collapsed[item.id] ? "Expand" : "Collapse"} onclick={() => (collapsed[item.id] = !collapsed[item.id])}>
                  <Icon name="chevron-right" size={10} stroke={2.4} />
                </button>
              {:else}
                <span class="chevron"></span>
              {/if}
              <button class="title" title={item.title} onclick={() => openItem(item)}>
                <span class="text">{item.title}</span>
                {#if item.page !== null}<span class="pg">{item.page}</span>{:else if item.url}<Icon name="external" size={11} />{/if}
              </button>
            </div>
          {/each}
        {/if}
      </div>
    </aside>
  {/if}

  {#if doc && searchOpen}
    <div class="find glass" role="search">
      <Icon name="search" size={14} />
      <input
        bind:this={searchInput}
        bind:value={query}
        placeholder="Find in PDF"
        aria-label="Find in PDF"
        oninput={() => searchSoon(query)}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            searchSoon.flush();
            step(e.shiftKey ? -1 : 1);
          }
        }}
      />
      <span class="count" aria-live="polite">
        {#if !query.trim()}
          {""}
        {:else if matches.length}
          {selected + 1} of {matches.length}{searching ? "+" : ""}
        {:else if searching}
          Searching…
        {:else if noText}
          No text
        {:else}
          Not found
        {/if}
      </span>
      <span class="nav">
        <button class="icon-btn" title="Previous match ({isMac ? "⇧⌘G" : "Shift+Enter"})" disabled={!matches.length} onclick={() => step(-1)}><Icon name="chevron-left" size={14} stroke={2} /></button>
        <button class="icon-btn" title="Next match ({isMac ? "⌘G" : "Enter"})" disabled={!matches.length} onclick={() => step(1)}><Icon name="chevron-right" size={14} stroke={2} /></button>
      </span>
      <button class="icon-btn" title="Done (Esc)" onclick={closeSearch}><Icon name="xmark" size={13} stroke={2} /></button>
    </div>
    {#if noText && !searching}
      <p class="find-note glass">This PDF has no selectable text — scanned pages can’t be searched.</p>
    {/if}
  {/if}

  {#if doc}
    <div class="controls glass">
      <button class="icon-btn" class:active={searchOpen} title="Find in PDF ({isMac ? "⌘F" : "Ctrl+F"})" onclick={() => (searchOpen ? closeSearch() : openSearch())}>
        <Icon name="search" size={16} />
      </button>
      <span class="divider"></span>
      <button class="icon-btn" title="Zoom out" onclick={() => setScale(scale / 1.2)}><Icon name="zoom-out" size={17} /></button>
      <button class="zoom" title="Fit width" onclick={() => setScale(fitScale(), true)}>{Math.round(scale * 100)}%</button>
      <button class="icon-btn" title="Zoom in" onclick={() => setScale(scale * 1.2)}><Icon name="zoom-in" size={17} /></button>
      <span class="divider"></span>
      <button class="icon-btn" class:active={fit} title="Fit width" onclick={() => setScale(fitScale(), true)}>
        <Icon name="fit-width" size={17} />
      </button>
      <span class="divider"></span>
      <input
        class="page-input"
        bind:value={pageInput}
        aria-label="Page"
        onkeydown={(e) => {
          if (e.key === "Enter") {
            const n = Math.max(1, Math.min(sizes.length, Number(pageInput) || 1));
            goTo(n);
            e.currentTarget.blur();
          }
        }}
      />
      <span class="total">of {sizes.length}</span>
    </div>
  {/if}
</div>

<style>
  .pdf {
    position: absolute;
    inset: 0;
    background: var(--window);
  }
  :global([data-appearance="light"]) .pdf {
    background: #e9e9ee;
  }
  .scroller {
    position: absolute;
    inset: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 16px 16px 90px;
  }
  .page {
    position: relative;
    flex: none;
    background: #fff;
    border-radius: 4px;
    box-shadow: 0 0 0 0.5px rgba(0, 0, 0, 0.08), 0 2px 8px rgba(0, 0, 0, 0.1), 0 12px 32px rgba(0, 0, 0, 0.08);
    overflow: hidden;
  }
  .page :global(canvas) {
    display: block;
  }
  .state {
    position: absolute;
    inset: 0;
    z-index: 2;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--label-3);
  }
  .controls {
    position: absolute;
    left: 50%;
    bottom: 18px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px 10px 4px 4px;
    border-radius: var(--r-pill);
    background: var(--glass-strong);
    box-shadow: var(--shadow-lg);
  }
  .controls .icon-btn {
    border-radius: 50%;
  }
  .zoom {
    min-width: 50px;
    height: 28px;
    border: 0;
    border-radius: var(--r-pill);
    background: transparent;
    color: var(--label);
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .zoom:hover {
    background: var(--fill-2);
  }
  .divider {
    width: 0.5px;
    height: 18px;
    margin: 0 4px;
    background: var(--separator-strong);
  }
  .page-input {
    width: 38px;
    height: 24px;
    border: 0;
    border-radius: 6px;
    background: var(--fill-2);
    text-align: center;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    outline: none;
  }
  .page-input:focus {
    box-shadow: 0 0 0 2px var(--accent-tint-strong);
  }
  /* Table of contents: a floating panel like the notes' outline panel; the
     pages, zoom bar and find bar make room for it. */
  .with-outline .scroller {
    right: 290px;
  }
  .with-outline .controls {
    left: calc((100% - 290px) / 2);
  }
  .with-outline .find,
  .with-outline .find-note {
    right: 306px;
  }
  .outline {
    position: absolute;
    top: 10px;
    right: 10px;
    bottom: 10px;
    z-index: 3;
    display: flex;
    flex-direction: column;
    width: 270px;
    border-radius: 18px;
    background: var(--glass-strong);
    box-shadow: var(--shadow-lg);
    animation: outline-in 220ms var(--ease-out);
  }
  @keyframes outline-in {
    from {
      opacity: 0;
      transform: translateX(12px);
    }
  }
  .outline header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 10px 6px 16px;
  }
  .outline h3 {
    margin: 0;
    font: 600 13px var(--font-display);
  }
  .outline header .icon-btn {
    width: 24px;
    height: 24px;
    border-radius: 50%;
  }
  .outline .list {
    flex: 1;
    overflow-y: auto;
    padding: 2px 8px 12px;
  }
  .outline .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 40px 16px;
    color: var(--label-3);
    text-align: center;
  }
  .outline .empty p {
    margin: 0;
    color: var(--label-2);
    font-size: 12px;
  }
  .entry {
    display: flex;
    align-items: center;
    border-radius: 7px;
  }
  .entry:hover {
    background: var(--fill-2);
  }
  .entry.current {
    background: var(--accent-tint);
  }
  .entry .chevron {
    display: grid;
    place-items: center;
    flex: none;
    width: 16px;
    height: 16px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--label-3);
    transition: transform var(--dur) var(--ease);
  }
  .entry .chevron.open {
    transform: rotate(90deg);
  }
  .entry .title {
    flex: 1;
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    padding: 5px 8px 5px 4px;
    border: 0;
    background: none;
    color: var(--label);
    font-size: 12.5px;
    line-height: 1.35;
    text-align: left;
  }
  .entry .text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .entry.current .title {
    color: var(--accent);
    font-weight: 600;
  }
  .entry.bold .text {
    font-weight: 600;
  }
  .entry.italic .text {
    font-style: italic;
  }
  .entry.dead .title {
    color: var(--label-3);
    cursor: default;
  }
  .entry .pg {
    flex: none;
    color: var(--label-3);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .find {
    position: absolute;
    top: 12px;
    right: 16px;
    z-index: 3;
    display: flex;
    align-items: center;
    gap: 4px;
    width: min(360px, calc(100% - 32px));
    height: 36px;
    padding: 0 4px 0 12px;
    border-radius: var(--r-pill);
    background: var(--glass-strong);
    box-shadow: var(--shadow-lg);
    color: var(--label-3);
    animation: find-in 160ms var(--ease-out);
  }
  @keyframes find-in {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
  }
  .find input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: none;
    color: var(--label);
    font: inherit;
    font-size: 13px;
  }
  .find .count {
    flex: none;
    color: var(--label-2);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .find .nav {
    display: flex;
  }
  .find .icon-btn {
    width: 26px;
    height: 26px;
    border-radius: 50%;
  }
  .find .icon-btn:disabled {
    opacity: 0.35;
  }
  .find-note {
    position: absolute;
    top: 56px;
    right: 16px;
    z-index: 3;
    margin: 0;
    padding: 8px 14px;
    border-radius: var(--r);
    background: var(--glass-strong);
    box-shadow: var(--shadow);
    color: var(--label-2);
    font-size: 12px;
  }
  /* Search highlights: yellow like Preview, the current match in orange. */
  .page :global(.textLayer .highlight) {
    --highlight-bg-color: rgb(255 214 10 / 0.45);
    --highlight-selected-bg-color: rgb(255 149 0 / 0.65);
  }
  .total {
    margin-left: 6px;
    color: var(--label-2);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
</style>
