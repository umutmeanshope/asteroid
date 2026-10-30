<script lang="ts">
  import { api } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import type { SizeReport } from "../lib/types";
  import { formatBytes } from "../lib/util";
  import Icon, { type IconName } from "./Icon.svelte";

  // File types, files and folders kept out of git sync. Rendered as rows of
  // the surrounding Settings group.
  const presets: { label: string; exts: string[]; icon: IconName; note?: string }[] = [
    { label: "PDF documents", exts: ["pdf"], icon: "pdf" },
    { label: "Word documents", exts: ["doc", "docx", "odt", "rtf"], icon: "file" },
    { label: "Spreadsheets", exts: ["xls", "xlsx", "ods"], icon: "file" },
    { label: "Presentations", exts: ["ppt", "pptx", "odp", "key"], icon: "file" },
    { label: "Images", exts: ["png", "jpg", "jpeg", "gif", "webp", "heic", "tif", "tiff", "bmp"], icon: "image", note: "Images embedded in notes won’t show on other devices" },
    { label: "Video", exts: ["mp4", "mov", "mkv", "webm", "avi"], icon: "file" },
    { label: "Audio", exts: ["mp3", "m4a", "wav", "flac", "ogg"], icon: "file" },
    { label: "Archives", exts: ["zip", "rar", "7z", "tar", "gz"], icon: "file" },
  ];
  const presetExts = new Set(presets.flatMap((p) => p.exts));

  let report = $state<SizeReport | null>(null);
  let custom = $state("");

  $effect(() => {
    void app.ignoreVersion;
    void api.gitSizeReport().then((r) => (report = r)).catch(() => {});
  });

  let customExts = $derived(app.ignore.extensions.filter((e) => !presetExts.has(e)));

  function usage(exts: string[]): string {
    if (!report) return exts.map((e) => `.${e}`).join(" ");
    let count = 0;
    let bytes = 0;
    for (const s of report.byExtension) {
      if (exts.includes(s.ext)) {
        count += s.count;
        bytes += s.bytes;
      }
    }
    return count ? `${count} file${count === 1 ? "" : "s"} · ${formatBytes(bytes)}` : "None in this vault";
  }

  const isOn = (exts: string[]) => exts.every((e) => app.ignore.extensions.includes(e));

  function setExts(exts: string[], on: boolean) {
    const rest = app.ignore.extensions.filter((e) => !exts.includes(e));
    void app.setIgnoreRules({ ...app.ignore, extensions: on ? [...rest, ...exts] : rest });
  }

  function addCustom() {
    const ext = custom.trim().replace(/^\*?\./, "").toLowerCase().replace(/[^a-z0-9]/g, "");
    custom = "";
    if (ext && !app.ignore.extensions.includes(ext)) setExts([ext], true);
  }

  /** Biggest extensions in the vault not covered above, as one-click suggestions. */
  let suggestions = $derived(
    (report?.byExtension ?? [])
      .filter((s) => s.ext && s.ext !== "md" && !presetExts.has(s.ext) && !app.ignore.extensions.includes(s.ext) && s.bytes > 5 * 1024 * 1024)
      .slice(0, 4),
  );
</script>

{#each presets as preset (preset.label)}
  {@const on = isOn(preset.exts)}
  <div class="row">
    <span class="tile" class:on><Icon name={preset.icon} size={14} stroke={1.7} /></span>
    <span class="label">
      {preset.label}
      <small>{usage(preset.exts)}{on && preset.note ? ` — ${preset.note}` : ""}</small>
    </span>
    <button class="switch" class:on aria-label="Don’t sync {preset.label}" onclick={() => setExts(preset.exts, !on)}></button>
  </div>
{/each}

<div class="row wrap">
  <span class="label">Other file types<small>Extensions to leave out, like psd or sqlite</small></span>
  <div class="chips">
    {#each customExts as ext (ext)}
      <span class="chip">.{ext}<button aria-label="Sync .{ext} again" onclick={() => setExts([ext], false)}><Icon name="xmark" size={10} stroke={2.4} /></button></span>
    {/each}
    {#each suggestions as s (s.ext)}
      <button class="chip suggest" title="{s.count} files · {formatBytes(s.bytes)}" onclick={() => setExts([s.ext], true)}>
        <Icon name="plus" size={10} stroke={2.4} />.{s.ext}
      </button>
    {/each}
    <input
      class="field add"
      placeholder="Add…"
      bind:value={custom}
      onkeydown={(e) => e.key === "Enter" && addCustom()}
      onblur={() => custom.trim() && addCustom()}
    />
  </div>
</div>

{#if app.ignore.paths.length}
  {#each app.ignore.paths as p (p)}
    <div class="row">
      <span class="tile"><Icon name={p.endsWith("/") ? "folder" : "file"} size={14} stroke={1.7} /></span>
      <span class="label path">{p.replace(/\/$/, "")}<small>{p.endsWith("/") ? "Folder and everything in it" : "File"}</small></span>
      <button class="btn" onclick={() => void app.toggleExcluded(p.replace(/\/$/, ""), p.endsWith("/"))}>Sync Again</button>
    </div>
  {/each}
{:else}
  <div class="row hint">
    <Icon name="cloud-off" size={14} />
    <span>To leave out a single file or folder, right-click it in the sidebar and choose <strong>Don’t Sync</strong>.</span>
  </div>
{/if}

<style>
  .tile {
    display: grid;
    place-items: center;
    flex: none;
    width: 24px;
    height: 24px;
    border-radius: 7px;
    background: var(--fill-2);
    color: var(--label-2);
    transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
  }
  .tile.on {
    background: var(--fill);
    color: var(--label-3);
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row.wrap {
    flex-wrap: wrap;
    align-items: flex-start;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: 6px;
    max-width: 60%;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 24px;
    padding: 0 6px 0 9px;
    border: 0;
    border-radius: var(--r-pill);
    background: var(--fill);
    color: var(--label);
    font: 500 12px var(--font-mono);
  }
  .chip button {
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--label-2);
  }
  .chip button:hover {
    background: var(--fill);
  }
  .chip.suggest {
    padding: 0 9px 0 7px;
    background: transparent;
    box-shadow: inset 0 0 0 1px var(--separator-strong);
    color: var(--label-2);
  }
  .chip.suggest:hover {
    background: var(--fill-2);
  }
  .field.add {
    width: 76px;
    height: 24px;
    font-size: 12px;
  }
  .hint {
    gap: 8px;
    color: var(--label-2);
    font-size: 12px;
  }
</style>
