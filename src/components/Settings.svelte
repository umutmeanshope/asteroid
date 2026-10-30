<script lang="ts">
  import { api, message } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import type { Accent, DriveFolder, DriveStatus, GithubRepo, GithubStatus } from "../lib/types";
  import { modKey, relativeTime, syncTargetName } from "../lib/util";
  import DriveBrowser from "./DriveBrowser.svelte";
  import GithubConnect from "./GithubConnect.svelte";
  import GitExclusions from "./GitExclusions.svelte";
  import GitSizeNotice from "./GitSizeNotice.svelte";
  import RepoPicker from "./RepoPicker.svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import Sheet from "./Sheet.svelte";

  const sections: { id: typeof app.settingsSection; label: string; icon: IconName; color: string }[] = [
    { id: "general", label: "General", icon: "gear", color: "#8e8e93" },
    { id: "editor", label: "Editor", icon: "pencil", color: "#007aff" },
    { id: "sync", label: "Sync", icon: "cloud", color: "#34c759" },
    { id: "about", label: "About", icon: "sparkle", color: "#af52de" },
  ];
  const accents: Accent[] = ["blue", "purple", "pink", "red", "orange", "yellow", "green", "graphite"];

  let drive = $state<DriveStatus | null>(null);
  let github = $state<GithubStatus | null>(null);
  let githubConnecting = $state(false);
  /** Which picker is open for linking the current vault. */
  let linking = $state<null | "drive" | "github">(null);
  let busy = $state(false);

  async function refreshDrive() {
    drive = await api.driveStatus().catch(() => null);
  }
  async function refreshGithub() {
    github = await api.githubStatus().catch(() => null);
  }
  $effect(() => {
    void refreshDrive();
    void refreshGithub();
  });

  async function disconnectGithub() {
    await api.githubDisconnect();
    await refreshGithub();
  }

  async function linkRepo(repo: GithubRepo) {
    try {
      const l = await api.gitLink(repo);
      app.setVaultSync(l);
      linking = null;
      void app.syncNow();
    } catch (e) {
      app.toast(message(e), "error");
    }
  }

  async function connect() {
    busy = true;
    try {
      const email = await api.driveConnect();
      app.toast(`Connected as ${email}`, "success");
    } catch (e) {
      app.toast(message(e), "error");
    } finally {
      busy = false;
      await refreshDrive();
    }
  }

  async function disconnect() {
    await api.driveDisconnect();
    await refreshDrive();
  }

  async function link(folder: DriveFolder) {
    try {
      const l = await api.syncLink(folder.id, folder.name);
      app.setVaultSync(l);
      linking = null;
      void app.syncNow();
    } catch (e) {
      app.toast(message(e), "error");
    }
  }

  async function createAndLink() {
    if (!app.vault) return;
    busy = true;
    try {
      const folder = await api.driveCreateFolder(app.vault.name);
      await link(folder);
    } catch (e) {
      app.toast(message(e), "error");
    } finally {
      busy = false;
    }
  }

  async function unlink() {
    await api.syncUnlink();
    app.setVaultSync(null);
  }

  const shortcuts: [string, string][] = [
    ["Quick switcher", `${modKey} O`],
    ["Command palette", `${modKey} P`],
    ["New note", `${modKey} N`],
    ["Toggle reading view", `${modKey} E`],
    ["Search vault", `${modKey} ⇧ F`],
    ["Bold / Italic", `${modKey} B / ${modKey} I`],
    ["Link selection", `${modKey} K`],
    ["Toggle checkbox", `${modKey} ↵`],
    ["Find in note", `${modKey} F`],
    ["Back / Forward", `${modKey} [ / ${modKey} ]`],
  ];
</script>

<Sheet label="Settings" placement="center" width={780} onclose={() => (app.modal = null)}>
  <div class="settings">
    <nav>
      <h1>Settings</h1>
      {#each sections as s (s.id)}
        <button class:on={app.settingsSection === s.id} onclick={() => (app.settingsSection = s.id)}>
          <span class="tile" style:background={s.color}><Icon name={s.icon} size={13} stroke={2} /></span>
          {s.label}
        </button>
      {/each}
    </nav>

    <section>
      <button class="icon-btn close" aria-label="Close" onclick={() => (app.modal = null)}><Icon name="xmark" size={14} /></button>

      {#if app.settingsSection === "general"}
        <h2>General</h2>
        <div class="group">
          <div class="row">
            <span class="label">Appearance</span>
            <div class="segmented">
              {#each [["system", "Auto"], ["light", "Light"], ["dark", "Dark"]] as [value, label]}
                <button
                  class:on={app.settings.theme === value}
                  onclick={() => app.updateSettings({ theme: value as "system" | "light" | "dark" })}>{label}</button
                >
              {/each}
            </div>
          </div>
          <div class="row">
            <span class="label">Accent color</span>
            <div class="swatches">
              {#each accents as accent}
                <button
                  class="swatch"
                  class:on={app.settings.accent === accent}
                  style:background="var(--{accent})"
                  aria-label={accent}
                  title={accent}
                  onclick={() => app.updateSettings({ accent })}
                ></button>
              {/each}
            </div>
          </div>
        </div>

        <div class="group-title">Files</div>
        <div class="group">
          <div class="row">
            <span class="label">Ask before deleting<small>Deleted files go to the vault’s <code>.trash</code> folder.</small></span>
            <button
              class="switch"
              class:on={app.settings.confirmDelete}
              aria-label="Ask before deleting"
              onclick={() => app.updateSettings({ confirmDelete: !app.settings.confirmDelete })}
            ></button>
          </div>
        </div>
      {:else if app.settingsSection === "editor"}
        <h2>Editor</h2>
        <div class="group">
          <div class="row">
            <span class="label">Live preview<small>Hide Markdown syntax except on the line you’re editing.</small></span>
            <button
              class="switch"
              class:on={app.settings.livePreview}
              aria-label="Live preview"
              onclick={() => app.updateSettings({ livePreview: !app.settings.livePreview })}
            ></button>
          </div>
          <div class="row">
            <span class="label">Open notes in</span>
            <div class="segmented">
              <button class:on={app.settings.defaultMode === "edit"} onclick={() => app.updateSettings({ defaultMode: "edit" })}>Editing</button>
              <button class:on={app.settings.defaultMode === "read"} onclick={() => app.updateSettings({ defaultMode: "read" })}>Reading</button>
            </div>
          </div>
          <div class="row">
            <span class="label">Text size</span>
            <input
              type="range"
              min="13"
              max="22"
              step="1"
              value={app.settings.fontSize}
              oninput={(e) => app.updateSettings({ fontSize: Number(e.currentTarget.value) })}
            />
            <span class="value">{app.settings.fontSize} pt</span>
          </div>
          <div class="row">
            <span class="label">Readable line length<small>Limit text width for comfortable reading.</small></span>
            <button
              class="switch"
              class:on={app.settings.readableWidth}
              aria-label="Readable line length"
              onclick={() => app.updateSettings({ readableWidth: !app.settings.readableWidth })}
            ></button>
          </div>
          {#if app.settings.readableWidth}
            <div class="row">
              <span class="label">Line width</span>
              <input
                type="range"
                min="50"
                max="140"
                step="5"
                aria-label="Line width"
                value={app.settings.lineWidth}
                oninput={(e) => app.updateSettings({ lineWidth: Number(e.currentTarget.value) })}
              />
              <span class="value">{app.settings.lineWidth} ch</span>
            </div>
          {/if}
          <div class="row">
            <span class="label">Update links when renaming<small>Keep links to a note working when it’s renamed or moved.</small></span>
            <button
              class="switch"
              class:on={app.settings.updateLinks}
              aria-label="Update links when renaming"
              onclick={() => app.updateSettings({ updateLinks: !app.settings.updateLinks })}
            ></button>
          </div>
          <div class="row">
            <span class="label">Check spelling</span>
            <button
              class="switch"
              class:on={app.settings.spellcheck}
              aria-label="Check spelling"
              onclick={() => app.updateSettings({ spellcheck: !app.settings.spellcheck })}
            ></button>
          </div>
        </div>

        <div class="group-title">Keyboard shortcuts</div>
        <div class="group">
          {#each shortcuts as [label, keys]}
            <div class="row compact"><span class="label">{label}</span><span class="kbd">{keys}</span></div>
          {/each}
        </div>
      {:else if app.settingsSection === "sync"}
        <h2>Sync</h2>
        <div class="group-title">Accounts</div>
        <div class="group">
          <div class="row">
            <span class="tile big" style:background="linear-gradient(160deg,#34c759,#0a9b41)"><Icon name="drive" size={18} stroke={1.6} /></span>
            <span class="label">
              Google Drive
              <small>
                {#if !drive}Checking…{:else if drive.connected}Connected{drive.email ? ` as ${drive.email}` : ""}{:else if drive.configured}Not connected{:else}Needs an OAuth client ID (below){/if}
              </small>
            </span>
            {#if drive?.connected}
              <button class="btn" onclick={disconnect}>Disconnect</button>
            {:else}
              <button class="btn primary" disabled={!drive?.configured || busy} onclick={connect}>
                {#if busy}<span class="spinner"></span>{/if} Connect
              </button>
            {/if}
          </div>
          <div class="row">
            <span class="tile big" style:background="linear-gradient(160deg,#48484a,#1c1c1e)"><Icon name="git" size={18} stroke={1.7} /></span>
            <span class="label">
              GitHub
              <small>
                {#if !github}Checking…{:else if github.connected}Connected as {github.login}{:else if github.configured}Not connected{:else}Needs an OAuth client ID (below){/if}
              </small>
            </span>
            {#if github?.connected}
              <button class="btn" onclick={disconnectGithub}>Disconnect</button>
            {:else if !githubConnecting}
              <button class="btn primary" disabled={!github?.configured} onclick={() => (githubConnecting = true)}>Connect</button>
            {/if}
          </div>
          {#if githubConnecting}
            <div class="row block">
              <GithubConnect
                onconnected={(login) => {
                  githubConnecting = false;
                  app.toast(`Connected to GitHub as ${login}`, "success");
                  void refreshGithub();
                }}
                oncancel={() => (githubConnecting = false)}
              />
            </div>
          {/if}
        </div>

        {#if app.vault}
          <div class="group-title">This vault</div>
          <div class="group">
            {#if app.vault.sync}
              <div class="row">
                <span class="label">
                  Synced with “{syncTargetName(app.vault.sync)}”
                  {#if app.vault.sync.provider === "git"}on GitHub{/if}
                  <small>
                    {#if app.sync.running}Syncing now…{:else if app.sync.error}<span class="err">{app.sync.error}</span>{:else if app.sync.lastAt}Last synced {relativeTime(app.sync.lastAt)}{:else}Not synced yet{/if}
                  </small>
                </span>
                <button class="btn" disabled={app.sync.running} onclick={() => app.syncNow()}>Sync Now</button>
                <button class="btn danger" onclick={unlink}>Unlink</button>
              </div>
              <div class="row">
                <span class="label">Sync automatically</span>
                <div class="segmented">
                  {#each [[0, "Off"], [1, "1 min"], [5, "5 min"], [15, "15 min"]] as [value, label]}
                    <button
                      class:on={app.settings.autoSyncMinutes === value}
                      onclick={() => app.updateSettings({ autoSyncMinutes: value as number })}>{label}</button
                    >
                  {/each}
                </div>
              </div>
              {#if app.vault.sync.provider === "git"}
                <div class="row block"><GitSizeNotice linked /></div>
              {/if}
            {:else if linking === "drive"}
              <div class="row block">
                <DriveBrowser action={(name) => `Sync with “${name}”`} onpick={link} />
                <button class="btn back" onclick={() => (linking = null)}>Cancel</button>
              </div>
            {:else if linking === "github"}
              <div class="row block">
                <div class="size-before"><GitSizeNotice linked={false} /></div>
                <RepoPicker
                  allowCreate
                  suggestedName={app.vault.name}
                  action={(repo) => `Sync with “${repo.name}”`}
                  onpick={linkRepo}
                />
                <button class="btn back" onclick={() => (linking = null)}>Cancel</button>
              </div>
            {:else}
              <div class="row">
                <span class="tile" style:background="linear-gradient(160deg,#34c759,#0a9b41)"><Icon name="drive" size={15} stroke={1.6} /></span>
                <span class="label">Google Drive<small>Files sync as they are; edits on two devices keep both copies.</small></span>
                <button class="btn" disabled={!drive?.connected || busy} onclick={createAndLink}>Create “{app.vault.name}”</button>
                <button class="btn" disabled={!drive?.connected} onclick={() => (linking = "drive")}>Choose Folder…</button>
              </div>
              <div class="row">
                <span class="tile" style:background="linear-gradient(160deg,#48484a,#1c1c1e)"><Icon name="git" size={15} stroke={1.7} /></span>
                <span class="label">GitHub<small>Keeps full history; edits to different parts of a note merge.</small></span>
                <button class="btn" disabled={!github?.connected} onclick={() => (linking = "github")}>Choose Repository…</button>
              </div>
            {/if}
          </div>
          <p class="group-footer">
            {#if app.vault.sync?.provider === "git"}
              Every sync commits all changes in the vault, then merges what other devices pushed. When two devices
              change the same lines, the other version is kept as a “conflict” copy next to the note.
            {:else}
              Edits made on two devices at once are never merged silently — the other version is kept as a
              “conflict” copy next to the note. Deleted files go to the trash on both sides.
            {/if}
          </p>
        {/if}

        {#if app.vault && (app.usesGit || linking === "github")}
          <div class="group-title">Don’t sync</div>
          <div class="group">
            <GitExclusions />
          </div>
          <p class="group-footer">
            Left-out files stay where they are on every device but stop syncing, and new devices won’t receive
            them. Anything already uploaded stays in the repository’s history.
          </p>
        {/if}

        <div class="group-title">Developer credentials</div>
        <div class="group">
          <div class="row">
            <span class="label">Google client ID</span>
            <input
              class="field wide"
              placeholder="1234-abc.apps.googleusercontent.com"
              value={app.settings.googleClientId}
              onchange={async (e) => {
                await app.updateSettings({ googleClientId: e.currentTarget.value.trim() });
                await refreshDrive();
              }}
            />
          </div>
          <div class="row">
            <span class="label">Google client secret</span>
            <input
              class="field wide"
              type="password"
              placeholder="Optional for Desktop clients"
              value={app.settings.googleClientSecret}
              onchange={(e) => app.updateSettings({ googleClientSecret: e.currentTarget.value.trim() })}
            />
          </div>
          <div class="row">
            <span class="label">GitHub client ID</span>
            <input
              class="field wide"
              placeholder="Ov23li…"
              value={app.settings.githubClientId}
              onchange={async (e) => {
                await app.updateSettings({ githubClientId: e.currentTarget.value.trim() });
                await refreshGithub();
              }}
            />
          </div>
        </div>
        <p class="group-footer">
          Google: in Google Cloud Console, enable the Drive API and create an OAuth client of type
          <strong>Desktop app</strong>. GitHub: under Settings › Developer settings › OAuth Apps, create an app and
          tick <strong>Enable Device Flow</strong>; no secret is needed.
        </p>
      {:else}
        <div class="about">
          <img src="/icon.svg" alt="" width="96" height="96" />
          <h2>Asteroid</h2>
          <p>Version 0.1.0</p>
          <p class="muted">A light, fast home for your Markdown notes.<br />Built with Rust and Tauri.</p>
        </div>
      {/if}
    </section>
  </div>
</Sheet>

<style>
  .settings {
    display: flex;
    height: min(600px, calc(100vh - 48px));
  }
  nav {
    flex: none;
    width: 200px;
    padding: 18px 10px;
    background: var(--fill-3);
    border-right: 0.5px solid var(--separator);
  }
  nav h1 {
    margin: 4px 10px 14px;
    font: 700 13px var(--font-display);
    color: var(--label-2);
  }
  nav button {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    height: 32px;
    padding: 0 8px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    font-size: 13px;
    text-align: left;
  }
  nav button:hover {
    background: var(--fill-2);
  }
  nav button.on {
    background: var(--accent);
    color: #fff;
  }
  .tile {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    color: #fff;
    flex: none;
    box-shadow: 0 0.5px 1px rgba(0, 0, 0, 0.2);
  }
  .tile.big {
    width: 32px;
    height: 32px;
    border-radius: 8px;
  }
  section {
    position: relative;
    flex: 1;
    overflow-y: auto;
    padding: 22px 26px 28px;
    background: var(--window);
  }
  .close {
    position: absolute;
    top: 14px;
    right: 14px;
    border-radius: 50%;
    background: var(--fill-2);
    width: 26px;
    height: 26px;
  }
  h2 {
    margin: 0 0 14px;
    font: 700 22px var(--font-display);
    letter-spacing: -0.01em;
  }
  .row.compact {
    min-height: 36px;
  }
  .row.block {
    display: block;
    padding: 14px;
  }
  .btn.back {
    margin-top: 10px;
  }
  .size-before {
    margin-bottom: 14px;
  }
  .swatches {
    display: flex;
    gap: 8px;
  }
  .swatch {
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    box-shadow: inset 0 0 0 0.5px rgba(0, 0, 0, 0.15);
    transition: transform 150ms var(--ease);
  }
  .swatch:hover {
    transform: scale(1.12);
  }
  .swatch.on {
    outline: 2px solid var(--label-3);
    outline-offset: 2px;
  }
  input[type="range"] {
    width: 160px;
    accent-color: var(--accent);
  }
  .value {
    width: 42px;
    color: var(--label-2);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .field.wide {
    width: 300px;
  }
  .err {
    color: var(--red);
  }
  code {
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .about {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding-top: 40px;
    text-align: center;
  }
  .about img {
    filter: drop-shadow(0 8px 20px rgba(0, 0, 0, 0.2));
  }
  .about h2 {
    margin: 14px 0 2px;
  }
  .about p {
    margin: 2px;
    color: var(--label-2);
  }
  .about .muted {
    margin-top: 14px;
    color: var(--label-3);
    line-height: 1.5;
  }
</style>
