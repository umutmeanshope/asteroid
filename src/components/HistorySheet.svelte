<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import { api, message } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import type { DeletedFile, FileDiff, Version } from "../lib/types";
  import { fileName, parentOf, relativeTime, stem } from "../lib/util";
  import { renderMathIn } from "../lib/math";
  import Icon from "./Icon.svelte";
  import Sheet from "./Sheet.svelte";

  // Version history of one file (app.historyPath), or the vault's recently
  // deleted files when historyPath is null. Read-only until "Restore".
  const path = app.historyPath;
  const deletedMode = path === null;
  const isMarkdown = (p: string) => /\.(md|markdown)$/i.test(p);
  const isText = (p: string) => isMarkdown(p) || /\.(txt|csv|json|ya?ml|canvas|css|js|ts|html?)$/i.test(p);

  type View = "changes" | "compare" | "preview";

  let versions = $state<Version[]>([]);
  let deleted = $state<DeletedFile[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  /** Selected commit; "now" is the file on disk. */
  let selected = $state<string>("now");
  let selectedDeleted = $state<DeletedFile | null>(null);
  let view = $state<View>("changes");
  let diff = $state<FileDiff | null>(null);
  let preview = $state<{ html?: string; text?: string } | null>(null);
  let detailLoading = $state(false);
  let restoring = $state(false);

  let version = $derived(versions.find((v) => v.commit === selected) ?? null);
  let currentPath = $derived(deletedMode ? (selectedDeleted?.path ?? null) : path);

  async function load() {
    loading = true;
    try {
      if (deletedMode) {
        deleted = await api.gitDeletedFiles();
        selectedDeleted = deleted[0] ?? null;
        view = "preview";
      } else {
        versions = await api.gitFileHistory(path!);
      }
    } catch (e) {
      error = message(e);
    } finally {
      loading = false;
    }
  }
  load();

  // Load whatever the right-hand side shows for the current selection.
  $effect(() => {
    const target = currentPath;
    const commit = deletedMode ? selectedDeleted?.restoreFrom : selected;
    const mode = view;
    if (!target || !commit) return;
    let stale = false;
    detailLoading = true;
    void (async () => {
      try {
        if (mode === "preview") {
          diff = null;
          if (!isText(target)) preview = {};
          else {
            const text = commit === "now" ? await api.readNote(target) : await api.gitVersionText(target, commit);
            preview = isMarkdown(target) ? { html: await api.renderNote(target, text) } : { text };
          }
        } else {
          preview = null;
          // "now" + changes: what changed on this device since the last sync.
          const base = commit === "now" ? "HEAD" : commit;
          const against = mode === "compare" || commit === "now" ? "working" : "previous";
          diff = await api.gitVersionDiff(target, base, against);
        }
      } catch (e) {
        if (!stale) {
          diff = null;
          preview = { text: message(e) };
        }
      } finally {
        if (!stale) detailLoading = false;
      }
    })();
    return () => (stale = true);
  });

  /** Our sync's commit messages, made friendly. */
  function describe(v: Version): string {
    const device = /changed on (.+?)(?: \(part \d+\/\d+\))?$/.exec(v.message)?.[1];
    const merged = /^Asteroid: merge changes from (.+)$/.exec(v.message)?.[1];
    if (v.kind === "added") return device ? `Created on ${device}` : "Created";
    if (v.kind === "deleted") return device ? `Deleted on ${device}` : "Deleted";
    if (merged) return `Merged with changes from ${merged}`;
    if (device) return `Edited on ${device}`;
    return v.message || "Edited";
  }

  const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric" });
  const fullDayFmt = new Intl.DateTimeFormat(undefined, { month: "long", day: "numeric", year: "numeric" });
  const timeFmt = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" });

  function dayLabel(ms: number): string {
    const day = new Date(ms).setHours(0, 0, 0, 0);
    const today = new Date().setHours(0, 0, 0, 0);
    if (day === today) return "Today";
    if (day === today - 86_400_000) return "Yesterday";
    return new Date(ms).getFullYear() === new Date().getFullYear() ? dayFmt.format(ms) : fullDayFmt.format(ms);
  }

  let groups = $derived.by(() => {
    const out: { label: string; items: Version[] }[] = [];
    for (const v of versions) {
      const label = dayLabel(v.timeMs);
      if (out.at(-1)?.label !== label) out.push({ label, items: [] });
      out.at(-1)!.items.push(v);
    }
    return out;
  });

  let title = $derived.by(() => {
    if (deletedMode) return selectedDeleted ? fileName(selectedDeleted.path) : "";
    if (!version) return "Current Version";
    return `${dayLabel(version.timeMs)}, ${timeFmt.format(version.timeMs)}`;
  });

  /** Typesets math in a rendered preview whenever its HTML changes. */
  function typeset(node: HTMLElement, _html: string) {
    renderMathIn(node);
    return { update: () => renderMathIn(node) };
  }

  let canRestore = $derived(deletedMode ? !!selectedDeleted : !!version && version.kind !== "deleted");

  async function restore() {
    const target = currentPath;
    const commit = deletedMode ? selectedDeleted?.restoreFrom : version?.commit;
    if (!target || !commit) return;
    restoring = true;
    try {
      await app.flushAll();
      if (!deletedMode) {
        // Edits not yet synced aren't in history, so restoring would lose them.
        const unsynced = await api.gitVersionDiff(target, "HEAD", "working").catch(() => null);
        if (unsynced && unsynced.additions + unsynced.deletions > 0) {
          const ok = await ask(
            `“${stem(target)}” has edits that haven’t synced yet, so they aren’t in its history. Restoring replaces them.`,
            { title: "Replace unsynced edits?", kind: "warning", okLabel: "Restore", cancelLabel: "Cancel" },
          );
          if (!ok) return;
        }
      }
      await api.gitRestoreVersion(target, commit);
      const when = deletedMode ? relativeTime(selectedDeleted!.timeMs) : title;
      app.toast(deletedMode ? `Restored “${stem(target)}”` : `Restored “${stem(target)}” to ${when}`, "success");
      app.modal = null;
      if (deletedMode) app.openFile(target);
      else await app.refresh();
    } catch (e) {
      app.toast(message(e), "error");
    } finally {
      restoring = false;
    }
  }
</script>

<Sheet label={deletedMode ? "Recently deleted" : "Version history"} placement="center" width={980} onclose={() => !restoring && (app.modal = null)}>
  <div class="history">
    <aside class="timeline">
      <header>
        <span class="badge"><Icon name={deletedMode ? "trash" : "clock"} size={17} stroke={1.7} /></span>
        <div>
          <h2>{deletedMode ? "Recently Deleted" : "Version History"}</h2>
          <p title={path ?? ""}>{deletedMode ? "Files git can bring back" : stem(path!)}</p>
        </div>
      </header>
      <div class="list">
        {#if loading}
          <div class="center"><span class="spinner"></span></div>
        {:else if error}
          <div class="center err">{error}</div>
        {:else if deletedMode}
          {#each deleted as d (d.path)}
            <button class="item" class:on={selectedDeleted?.path === d.path} onclick={() => (selectedDeleted = d)}>
              <Icon name={isMarkdown(d.path) ? "note" : "file"} size={15} />
              <span class="text">
                <strong>{stem(d.path)}</strong>
                <small>{parentOf(d.path) ? `${parentOf(d.path)} · ` : ""}deleted {relativeTime(d.timeMs)}</small>
              </span>
            </button>
          {:else}
            <div class="center">Nothing deleted yet</div>
          {/each}
        {:else}
          <button class="item now" class:on={selected === "now"} onclick={() => (selected = "now")}>
            <span class="dot"></span>
            <span class="text"><strong>Current Version</strong><small>As it is on this device</small></span>
          </button>
          {#each groups as group (group.label)}
            <div class="day">{group.label}</div>
            {#each group.items as v (v.commit)}
              <button class="item" class:on={selected === v.commit} onclick={() => (selected = v.commit)}>
                <span class="dot" class:added={v.kind === "added"} class:deleted={v.kind === "deleted"}></span>
                <span class="text">
                  <strong>{timeFmt.format(v.timeMs)}</strong>
                  <small>{describe(v)}</small>
                </span>
                {#if v.additions !== null && (v.additions || v.deletions)}
                  <span class="stat">
                    {#if v.additions}<b class="plus">+{v.additions}</b>{/if}
                    {#if v.deletions}<b class="minus">−{v.deletions}</b>{/if}
                  </span>
                {/if}
              </button>
            {/each}
          {:else}
            <div class="center small">No synced versions yet. History starts with the first git sync.</div>
          {/each}
        {/if}
      </div>
    </aside>

    <section class="detail">
      <div class="bar">
        <div class="heading">
          <strong>{title}</strong>
          {#if version}<small>{version.author}</small>{:else if !deletedMode}<small>Includes edits not synced yet</small>{:else if selectedDeleted}<small>Deleted by {selectedDeleted.author}</small>{/if}
        </div>
        {#if !deletedMode}
          <div class="segmented">
            <button class:on={view === "changes"} onclick={() => (view = "changes")}>Changes</button>
            <button class:on={view === "compare"} disabled={selected === "now"} onclick={() => (view = "compare")}>Compared to Now</button>
            <button class:on={view === "preview"} onclick={() => (view = "preview")}>Preview</button>
          </div>
        {/if}
        {#if canRestore}
          <button class="btn primary" disabled={restoring} onclick={restore}>
            {#if restoring}<span class="spinner"></span>{:else}<Icon name="restore" size={14} />{/if}
            {deletedMode ? "Restore File" : "Restore This Version"}
          </button>
        {/if}
        <button class="btn" onclick={() => !restoring && (app.modal = null)}>Done</button>
      </div>

      <div class="content">
        {#if detailLoading && !diff && !preview}
          <div class="center"><span class="spinner"></span></div>
        {:else if preview}
          {#if preview.html !== undefined}
            <article class="markdown preview" use:typeset={preview.html}>{@html preview.html}</article>
          {:else if preview.text !== undefined}
            <pre class="plain">{preview.text}</pre>
          {:else}
            <div class="center"><Icon name="file" size={30} stroke={1.3} /><p>No preview for this kind of file. Restoring still brings it back exactly.</p></div>
          {/if}
        {:else if diff}
          {#if diff.binary}
            <div class="center"><Icon name="file" size={30} stroke={1.3} /><p>Binary file — line changes can’t be shown.</p></div>
          {:else if !diff.hunks.length}
            <div class="center">
              <Icon name="check" size={28} stroke={1.6} />
              <p>{view === "compare" ? "Identical to the current version." : selected === "now" ? "No edits since the last sync." : "No text changes."}</p>
            </div>
          {:else}
            <div class="summary">
              <b class="plus">+{diff.additions}</b>
              <b class="minus">−{diff.deletions}</b>
              <span>{view === "compare" ? "from this version to now" : selected === "now" ? "since the last sync" : "in this version"}</span>
            </div>
            <div class="diff">
              {#each diff.hunks as hunk, h (h)}
                {#if h > 0}<div class="gap">⋯</div>{/if}
                {#each hunk.lines as line, i (i)}
                  <div class="line" class:add={line.kind === "+"} class:del={line.kind === "-"}>
                    <span class="no">{line.oldLine ?? ""}</span>
                    <span class="no">{line.newLine ?? ""}</span>
                    <span class="sign">{line.kind === " " ? "" : line.kind === "-" ? "−" : "+"}</span>
                    <span class="code">{line.text || " "}</span>
                  </div>
                {/each}
              {/each}
              {#if diff.truncated}<div class="gap">Long diff shortened</div>{/if}
            </div>
          {/if}
        {/if}
      </div>
    </section>
  </div>
</Sheet>

<style>
  .history {
    display: flex;
    height: min(660px, calc(100vh - 80px));
  }
  .timeline {
    display: flex;
    flex-direction: column;
    width: 280px;
    flex: none;
    border-right: 0.5px solid var(--separator);
    background: color-mix(in srgb, var(--fill-3) 60%, transparent);
  }
  header {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 18px 16px 12px;
  }
  .badge {
    display: grid;
    place-items: center;
    flex: none;
    width: 34px;
    height: 34px;
    border-radius: 9px;
    background: linear-gradient(160deg, #5e5ce6, #3634a3);
    color: #fff;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.15);
  }
  header h2 {
    margin: 0;
    font: 700 15px var(--font-display);
  }
  header p {
    margin: 1px 0 0;
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--label-2);
    font-size: 12px;
  }
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 0 8px 12px;
  }
  .day {
    margin: 12px 8px 4px;
    color: var(--label-3);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 44px;
    padding: 6px 10px;
    border: 0;
    border-radius: 9px;
    background: none;
    color: var(--label);
    text-align: left;
  }
  .item:hover {
    background: var(--fill-2);
  }
  .item.on {
    background: var(--accent);
    color: #fff;
  }
  .item.on small,
  .item.on .stat b {
    color: rgba(255, 255, 255, 0.85);
  }
  .item > :global(svg) {
    flex: none;
    color: var(--label-2);
  }
  .item.on > :global(svg) {
    color: #fff;
  }
  .dot {
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--label-3);
  }
  .now .dot {
    background: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 25%, transparent);
  }
  .dot.added {
    background: var(--green);
  }
  .dot.deleted {
    background: var(--red);
  }
  .item.on .dot {
    background: #fff;
    box-shadow: none;
  }
  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .text strong {
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .text small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--label-2);
    font-size: 11.5px;
  }
  .stat {
    display: flex;
    gap: 5px;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .plus {
    color: var(--green);
    font-weight: 600;
  }
  .minus {
    color: var(--red);
    font-weight: 600;
  }
  .detail {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--surface);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 16px 12px 18px;
    border-bottom: 0.5px solid var(--separator);
  }
  .heading {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .heading strong {
    font: 600 14px var(--font-display);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .heading small {
    color: var(--label-2);
    font-size: 11.5px;
  }
  .segmented button:disabled {
    opacity: 0.4;
  }
  .content {
    flex: 1;
    overflow: auto;
    position: relative;
  }
  .center {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    height: 100%;
    min-height: 120px;
    padding: 24px;
    color: var(--label-3);
    text-align: center;
  }
  .center p {
    margin: 0;
    max-width: 320px;
    color: var(--label-2);
  }
  .center.small {
    height: auto;
    font-size: 12px;
  }
  .center.err {
    color: var(--red);
  }
  .summary {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 18px 4px;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .summary span {
    color: var(--label-2);
  }
  .diff {
    padding: 6px 0 24px;
    font: 12px/1.6 var(--font-mono);
  }
  .line {
    display: grid;
    grid-template-columns: 40px 40px 18px 1fr;
    padding-right: 16px;
  }
  .line.add {
    background: color-mix(in srgb, var(--green) 13%, transparent);
  }
  .line.del {
    background: color-mix(in srgb, var(--red) 11%, transparent);
  }
  .no {
    padding-right: 8px;
    color: var(--label-4);
    text-align: right;
    user-select: none;
  }
  .sign {
    color: var(--label-3);
    user-select: none;
  }
  .add .sign {
    color: var(--green);
  }
  .del .sign {
    color: var(--red);
  }
  .code {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .gap {
    margin: 6px 0;
    padding: 2px 98px;
    background: var(--fill-3);
    color: var(--label-3);
    font-size: 11px;
  }
  .preview {
    max-width: 720px;
    margin: 0 auto;
    padding: 24px 32px 40vh;
  }
  .plain {
    margin: 0;
    padding: 20px 24px;
    font: 12px/1.6 var(--font-mono);
    white-space: pre-wrap;
  }
</style>
