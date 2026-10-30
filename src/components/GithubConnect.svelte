<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onDestroy } from "svelte";
  import { api, message } from "../lib/api";
  import { copyText } from "../lib/clipboard";
  import { app } from "../lib/state.svelte";
  import type { DeviceCode } from "../lib/types";
  import Icon from "./Icon.svelte";

  // GitHub device sign-in: show a short code, the user approves it on
  // github.com, and the app picks the token up by polling.
  let { onconnected, oncancel }: { onconnected: (login: string) => void; oncancel?: () => void } = $props();

  let code = $state<DeviceCode | null>(null);
  let error = $state<string | null>(null);
  let copied = $state(false);
  let waiting = false;

  async function start() {
    error = null;
    try {
      code = await api.githubDeviceStart();
      waiting = true;
      const login = await api.githubDevicePoll(code);
      waiting = false;
      onconnected(login);
    } catch (e) {
      waiting = false;
      const text = message(e);
      if (text !== "Cancelled") error = text;
      code = null;
    }
  }

  async function copyAndOpen() {
    if (!code) return;
    await copyText(code.userCode).catch(() => {});
    copied = true;
    await openUrl(code.verificationUri).catch((e) => app.toast(message(e), "error"));
  }

  function cancel() {
    if (waiting) void api.githubDeviceCancel();
    oncancel?.();
  }

  onDestroy(() => {
    if (waiting) void api.githubDeviceCancel();
  });

  start();
</script>

<div class="connect">
  {#if error}
    <Icon name="warning" size={26} stroke={1.5} />
    <p class="error">{error}</p>
    <div class="buttons">
      <button class="btn" onclick={cancel}>Close</button>
      <button class="btn primary" onclick={start}>Try Again</button>
    </div>
  {:else if !code}
    <span class="spinner"></span>
    <p>Contacting GitHub…</p>
  {:else}
    <p>Enter this code on GitHub to let Asteroid sync your repositories:</p>
    <button class="code" title="Copy code" onclick={() => code && copyText(code.userCode).then(() => (copied = true))}>
      {#each code.userCode as ch, i (i)}<span class:dash={ch === "-"}>{ch}</span>{/each}
    </button>
    <div class="buttons">
      <button class="btn" onclick={cancel}>Cancel</button>
      <button class="btn primary" onclick={copyAndOpen}>
        <Icon name="external" size={14} />
        {copied ? "Open GitHub Again" : "Copy Code & Open GitHub"}
      </button>
    </div>
    <p class="waiting"><span class="spinner"></span> Waiting for you to approve on github.com…</p>
  {/if}
</div>

<style>
  .connect {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 18px 12px 14px;
    text-align: center;
    color: var(--label-2);
  }
  p {
    margin: 0;
    max-width: 360px;
    line-height: 1.45;
  }
  .error {
    color: var(--red);
  }
  .code {
    display: flex;
    gap: 5px;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
  }
  .code span {
    display: grid;
    place-items: center;
    width: 30px;
    height: 40px;
    border-radius: 8px;
    background: var(--fill-2);
    color: var(--label);
    font: 600 20px var(--font-mono);
  }
  .code span.dash {
    width: 12px;
    background: none;
    color: var(--label-3);
  }
  .code:hover span:not(.dash) {
    background: var(--fill);
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
  .waiting {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--label-3);
    font-size: 12px;
  }
</style>
