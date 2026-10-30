<script lang="ts">
  import type { Tab } from "../lib/types";
  import NoteView from "./NoteView.svelte";
  import PdfViewer from "./PdfViewer.svelte";
  import ImageViewer from "./ImageViewer.svelte";
  import OtherFile from "./OtherFile.svelte";

  let { tab, active }: { tab: Tab; active: boolean } = $props();
</script>

{#if tab.kind === "note"}
  <NoteView {tab} {active} />
{:else if tab.kind === "pdf"}
  <PdfViewer path={tab.path} {active} />
{:else if tab.kind === "image"}
  <ImageViewer path={tab.path} />
{:else}
  <OtherFile path={tab.path} />
{/if}
