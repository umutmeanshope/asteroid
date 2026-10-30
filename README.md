# Asteroid

A light, fast Markdown notes app in the spirit of Obsidian: plain `.md` files in a
folder ("vault"), wikilinks, backlinks, live preview, a PDF reader, and sync through
Google Drive or GitHub. Rust core, Tauri 2 shell, Svelte 5 UI styled after Apple's design language.

## Run

```sh
npm install
npm run tauri dev      # development
npm run tauri build    # release bundles (AppImage/deb/rpm on Linux)
```

Requires Rust ≥ 1.90, Node ≥ 20 and the [Tauri Linux prerequisites](https://tauri.app/start/prerequisites/)
(WebKitGTK 4.1).

```sh
cargo test --workspace   # core, Drive sync and git sync tests
npm run check            # Svelte/TypeScript checks
```

## Google Drive setup

Sync uses your own Google OAuth client:

1. In [Google Cloud Console](https://console.cloud.google.com/), create a project and
   enable the **Google Drive API**.
2. Configure the OAuth consent screen and add your account as a test user.
3. Create an OAuth client ID of type **Desktop app**.
4. In Asteroid open **Settings › Sync** and paste the client ID (and secret), or set
   `ASTEROID_GOOGLE_CLIENT_ID` / `ASTEROID_GOOGLE_CLIENT_SECRET` at build or run time.
5. Click **Connect**: sign-in happens in your browser (PKCE, loopback redirect).

Then either link the open vault to a Drive folder (Settings › Sync) or use
**Open from Google Drive** on the welcome screen to download a vault onto a new device.

## Links, embeds and math

- **Renaming or moving** a file or folder rewrites links that would otherwise break —
  `[[wikilinks]]`, `![[embeds]]`, `[markdown](links.md)` and links in frontmatter — keeping
  each link's style, heading and alias. Links that still resolve (a bare `[[Note]]` after a
  folder move) are left as written; markdown links that were correct relative paths stay
  correct paths. Settings › Editor › *Update links when renaming* turns this off.
- **Embeds**: `![[Note]]`, `![[Note#Heading]]` (the section, with its subsections) and
  `![[Note#^block-id]]` render inline in reading view and live preview, up to three levels
  deep, with loops shown as links.
- **Math**: `$inline$` and `$$display$$` (inline, or on their own lines) are typeset with
  KaTeX, loaded only when a note has math. `$5 and $6` stays text (Pandoc's rules).

## GitHub setup

Git sync signs in with GitHub's device flow, which needs only a client ID:

1. On GitHub open **Settings › Developer settings › OAuth Apps › New OAuth App**. Any
   homepage and callback URL will do (the device flow doesn't use them).
2. Tick **Enable Device Flow** and save. No client secret is needed.
3. In Asteroid open **Settings › Sync** and paste the client ID, or set
   `ASTEROID_GITHUB_CLIENT_ID` at build or run time.
4. Click **Connect** next to GitHub, then enter the code shown on github.com.

Then link the open vault to a repository (an existing one, or a new private one created
from the picker), or use **Open from GitHub** on the welcome screen to clone a vault onto
a new device. The sign-in asks for the `repo` scope (read/write access to your
repositories, including private ones).

## Layout

```
crates/asteroid-core   Vault file ops, Markdown parsing, index (search, backlinks,
                        tags, link resolution), HTML rendering. No UI dependencies.
crates/asteroid-sync   Three-way sync engine + Google Drive backend.
crates/asteroid-git    Git sync (libgit2: commit, fetch, merge, push) + GitHub sign-in.
src-tauri               Thin Tauri shell: commands, file watcher, settings storage.
src                     Svelte UI: CodeMirror 6 live-preview editor, pdf.js viewer.
```

The core and sync crates (`asteroid-core`, `asteroid-sync`, `asteroid-git`) have no Tauri dependency so the planned iOS and Android apps
can reuse them; `src-tauri` is already set up as a mobile-capable library
(`tauri::mobile_entry_point`), and mobile icons are generated in `src-tauri/icons`.

### How sync works

Each sync compares, per file, the local content hash, the Drive `md5Checksum`, and the
hash recorded at the last successful sync (`.asteroid/sync-state.json`). One-sided
changes are copied; edits on both sides keep both versions (the local one is saved as
`Note (conflict YYYY-MM-DD HHMM).md`); deletions go to the vault's `.trash` locally and
to Drive's trash remotely. A sync that would delete more than half the vault is refused
unless confirmed.

### How git sync works

A git-synced vault is an ordinary repository. Each sync:

1. commits every change in the vault (`Asteroid: 3 files changed on <device>`),
2. fetches the branch from GitHub,
3. fast-forwards, or merges when both sides have new commits. Edits to different parts
   of a note merge automatically; when both sides changed the same lines, this device's
   version stays and the other is saved as `Note (conflict YYYY-MM-DD HHMM).md`. No
   conflict markers are ever written into notes. An edit beats a deletion.
4. pushes, retrying if another device pushed in the meantime.

`.asteroid/` and `.trash/` are kept out of commits through `.git/info/exclude`, so the
vault's own `.gitignore` is left alone. A folder that's already a repository keeps its
branch; one whose `origin` points somewhere else is refused rather than repointed.
HTTPS only for now (no SSH).

**Don't sync** (Settings › Sync, or right-click a file or folder): file types (PDF, Word,
spreadsheets, images, …), custom extensions, and single files or folders can be left out.
Asteroid keeps these rules in a marked block of the vault's `.gitignore` (case-insensitive
patterns; lines you wrote yourself are left alone), so every device applies them. Files
that were already synced stop being tracked but stay on disk — on the device that
excluded them and on every other device, where the incoming removal is recognised and the
local copy kept. They remain in the repository's history.

### Version history

Vaults that are git repositories get **Version History…** (toolbar ⋯ menu, a file's
context menu, or the command palette) and **Recently Deleted…** (vault menu). History
lists every synced version of a note with what it changed, compares any version with the
current one, previews it, and restores it. A restore is written as a normal edit, so the
next sync records it and it can itself be undone. Edits not yet synced aren't in history;
restoring over them asks first.

## Not yet done

- Mobile shells (iOS/Android) — Google sign-in will need a custom-scheme redirect
  instead of the desktop loopback flow (GitHub's device flow already works as is).
- Reading-view code highlighting, searching inside PDFs from the vault-wide search.
- Drive changes are found by listing the vault folder; very large vaults would benefit
  from the Drive Changes API.
- The full `drive` scope is used so existing folders are visible; a published app
  should move to `drive.file` + Google Picker to avoid restricted-scope review.

## License

[MIT](LICENSE). Asteroid is an independent project, not affiliated with Obsidian.
