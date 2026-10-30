<script lang="ts">
  import { documentDir } from "@tauri-apps/api/path";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { message } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import type { GithubRepo } from "../lib/types";
  import Icon from "./Icon.svelte";
  import RepoPicker from "./RepoPicker.svelte";
  import Sheet from "./Sheet.svelte";

  let busy = $state(false);

  async function clone(repo: GithubRepo) {
    const dest = await openDialog({
      directory: true,
      title: `Choose where to save “${repo.name}”`,
      defaultPath: await documentDir().catch(() => undefined),
    });
    if (typeof dest !== "string") return;
    busy = true;
    try {
      await app.cloneVault(repo, dest);
      app.modal = null;
      app.toast(`Downloaded “${repo.name}”`, "success");
    } catch (e) {
      app.toast(message(e), "error");
    } finally {
      busy = false;
    }
  }
</script>

<Sheet label="Open vault from GitHub" placement="center" width={560} onclose={() => !busy && (app.modal = null)}>
  <div class="pull">
    <header>
      <span class="badge"><Icon name="git" size={22} stroke={1.6} /></span>
      <div>
        <h2>Open Vault from GitHub</h2>
        <p>Choose the repository that holds your notes. It’s downloaded to this device and kept in sync.</p>
      </div>
    </header>
    <RepoPicker {busy} action={(repo) => (busy ? "Downloading…" : `Download “${repo.name}”`)} onpick={clone} />
  </div>
</Sheet>

<style>
  .pull {
    padding: 22px;
    overflow-y: auto;
  }
  header {
    display: flex;
    gap: 14px;
    margin-bottom: 18px;
  }
  .badge {
    display: grid;
    place-items: center;
    flex: none;
    width: 44px;
    height: 44px;
    border-radius: 12px;
    background: linear-gradient(160deg, #48484a, #1c1c1e);
    color: #fff;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.15);
  }
  h2 {
    margin: 0 0 4px;
    font: 700 17px var(--font-display);
  }
  p {
    margin: 0;
    color: var(--label-2);
    line-height: 1.45;
  }
</style>
