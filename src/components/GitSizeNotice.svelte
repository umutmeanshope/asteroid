<script lang="ts">
  import { api } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import type { SizeReport } from "../lib/types";
  import { formatBytes } from "../lib/util";
  import Icon from "./Icon.svelte";

  // GitHub's size limits applied to the open vault: before linking (what the
  // repository would hold) or once linked (what its history holds).
  let { linked }: { linked: boolean } = $props();

  let report = $state<SizeReport | null>(null);
  $effect(() => {
    void app.ignoreVersion;
    void api.gitSizeReport().then((r) => (report = r)).catch(() => {});
  });

  let size = $derived(report ? (linked ? Math.max(report.historyBytes, report.totalBytes) : report.totalBytes) : 0);
  let ratio = $derived(report ? size / report.recommendedRepoBytes : 0);
  let tooBig = $derived(report?.largeFiles.filter((f) => f.bytes > report!.maxFileBytes) ?? []);
  let large = $derived(report?.largeFiles.filter((f) => f.bytes <= report!.maxFileBytes) ?? []);
  let level = $derived(tooBig.length ? "red" : ratio > 1 ? "orange" : ratio > 0.5 ? "yellow" : "green");
</script>

{#if report}
  <div class="size">
    <div class="meter-row">
      <span class="label">
        {linked ? "Repository size" : "Vault size"}
        <small>
          {formatBytes(size)} of the 1 GB GitHub recommends · {report.fileCount} files
          {#if report.ignoredCount}· {report.ignoredCount} not synced ({formatBytes(report.ignoredBytes)}){/if}
        </small>
      </span>
      <div class="meter {level}" role="meter" aria-valuenow={Math.round(ratio * 100)} aria-valuemin={0} aria-valuemax={100} aria-label="Repository size">
        <span style:width="{Math.min(100, Math.max(2, ratio * 100))}%"></span>
      </div>
    </div>

    {#if tooBig.length}
      <div class="notice red">
        <Icon name="warning" size={15} stroke={1.8} />
        <div>
          <p>GitHub rejects files over {formatBytes(report.maxFileBytes)}, so {tooBig.length === 1 ? "this file isn’t" : "these files aren’t"} synced. Move {tooBig.length === 1 ? "it" : "them"} out of the vault or list {tooBig.length === 1 ? "it" : "them"} in <code>.gitignore</code>.</p>
          <ul>{#each tooBig as f (f.path)}<li><span>{f.path}</span><b>{formatBytes(f.bytes)}</b></li>{/each}</ul>
        </div>
      </div>
    {/if}
    {#if large.length}
      <div class="notice orange">
        <Icon name="warning" size={15} stroke={1.8} />
        <div>
          <p>GitHub accepts files up to {formatBytes(report.maxFileBytes)} but warns about anything over {formatBytes(report.warnFileBytes)}:</p>
          <ul>{#each large as f (f.path)}<li><span>{f.path}</span><b>{formatBytes(f.bytes)}</b></li>{/each}</ul>
        </div>
      </div>
    {/if}
    {#if ratio > 1}
      <div class="notice orange">
        <Icon name="warning" size={15} stroke={1.8} />
        <p>
          {#if linked}
            This repository is past GitHub’s recommended 1 GB, and GitHub may ask for it to be reduced past 5 GB.
          {:else}
            This vault is larger than the 1 GB GitHub recommends for a repository.
          {/if}
          Google Drive sync suits vaults full of PDFs better.
        </p>
      </div>
    {:else if ratio > 0.5}
      <div class="notice">
        <Icon name="sparkle" size={15} stroke={1.6} />
        <p>Every change to a PDF or image keeps a full copy in the history, so a git-synced vault only grows. Notes take almost no space.</p>
      </div>
    {/if}
  </div>
{/if}

<style>
  .size {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .meter-row {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .label {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .label small {
    color: var(--label-2);
    font-size: 11.5px;
  }
  .meter {
    width: 140px;
    height: 6px;
    border-radius: var(--r-pill);
    background: var(--fill);
    overflow: hidden;
  }
  .meter span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--green);
    transition: width 400ms var(--ease-out);
  }
  .meter.yellow span {
    background: var(--yellow);
  }
  .meter.orange span {
    background: var(--orange);
  }
  .meter.red span {
    background: var(--red);
  }
  .notice {
    display: flex;
    gap: 9px;
    padding: 9px 11px;
    border-radius: var(--r);
    background: var(--fill-3);
    color: var(--label-2);
    font-size: 12px;
    line-height: 1.45;
  }
  .notice > :global(svg) {
    flex: none;
    margin-top: 1px;
  }
  .notice.orange {
    background: color-mix(in srgb, var(--orange) 11%, transparent);
  }
  .notice.orange > :global(svg) {
    color: var(--orange);
  }
  .notice.red {
    background: color-mix(in srgb, var(--red) 10%, transparent);
  }
  .notice.red > :global(svg) {
    color: var(--red);
  }
  p {
    margin: 0;
  }
  ul {
    margin: 6px 0 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    gap: 10px;
    padding: 2px 0;
  }
  li span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--label);
  }
  li b {
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  code {
    font-family: var(--font-mono);
    font-size: 11px;
  }
</style>
