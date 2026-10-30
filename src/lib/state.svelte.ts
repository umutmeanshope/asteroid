import { listen } from "@tauri-apps/api/event";
import { ask, open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, message } from "./api";
import type {
  Entry,
  NoteSummary,
  Settings,
  SyncReport,
  Tab,
  TagCount,
  GithubRepo,
  IgnoreRules,
  VaultInfo,
  ViewMode,
} from "./types";
import { debounce, fileName, invalidateBlobUrl, kindOf, parentOf, uid } from "./util";

export const DEFAULT_SETTINGS: Settings = {
  theme: "system",
  accent: "blue",
  fontSize: 16,
  readableWidth: true,
  updateLinks: true,
  lineWidth: 80,
  livePreview: true,
  spellcheck: false,
  defaultMode: "edit",
  confirmDelete: true,
  autoSyncMinutes: 5,
  googleClientId: "",
  googleClientSecret: "",
  githubClientId: "",
};

export interface Toast {
  id: string;
  text: string;
  kind: "info" | "success" | "error";
  action?: { label: string; run: () => void };
}

type Modal = null | "switcher" | "palette" | "settings" | "drive-pull" | "git-pull" | "history";
type SidebarView = "files" | "search" | "tags";

interface SyncStatus {
  running: boolean;
  lastAt: number | null;
  error: string | null;
}

interface Workspace {
  tabs: { path: string; mode: ViewMode }[];
  active: number;
  expanded: Record<string, boolean>;
  sidebarOpen: boolean;
  rightOpen: boolean;
  pdfOutlineOpen?: boolean;
}

class AppState {
  ready = $state(false);
  vault = $state<VaultInfo | null>(null);
  tree = $state<Entry[]>([]);
  notes = $state<NoteSummary[]>([]);
  tags = $state<TagCount[]>([]);

  tabs = $state<Tab[]>([]);
  activeId = $state<string | null>(null);
  activeTab = $derived(this.tabs.find((t) => t.id === this.activeId) ?? null);

  expanded = $state<Record<string, boolean>>({});
  selected = $state<string | null>(null);
  renaming = $state<string | null>(null);
  sidebarOpen = $state(true);
  rightOpen = $state(false);
  /** The table-of-contents panel in PDF tabs. */
  pdfOutlineOpen = $state(false);
  sidebarView = $state<SidebarView>("files");
  searchQuery = $state("");
  modal = $state<Modal>(null);
  settingsSection = $state<"general" | "editor" | "sync" | "about">("general");
  settings = $state<Settings>({ ...DEFAULT_SETTINGS });
  dark = $state(false);
  toasts = $state<Toast[]>([]);
  sync = $state<SyncStatus>({ running: false, lastAt: null, error: null });
  /** "Don't sync" rules for git vaults; bumped `ignoreVersion` on change. */
  ignore = $state<IgnoreRules>({ extensions: [], paths: [] });
  ignoreVersion = $state(0);
  /** File shown in the history sheet; null shows recently deleted files. */
  historyPath = $state<string | null>(null);
  /** Name of the vault being opened, while a switch is in progress. */
  opening = $state<string | null>(null);
  /** Latest editor text per tab, for the outline panel. */
  liveText = $state<Record<string, string>>({});

  /** Tab ids whose new note should focus its title for renaming. */
  focusTitle = new Set<string>();
  private fileListeners = new Set<(paths: string[]) => void>();
  private flushers = new Set<() => Promise<void> | void>();
  private syncTimer: ReturnType<typeof setInterval> | undefined;
  private media = window.matchMedia("(prefers-color-scheme: dark)");

  async init() {
    try {
      const stored = await api.loadSettings();
      this.settings = { ...DEFAULT_SETTINGS, ...(stored as Partial<Settings>) };
    } catch {
      /* first run */
    }
    this.applyAppearance();
    this.media.addEventListener("change", () => this.applyAppearance());

    await listen<{ paths: string[] }>("vault-changed", (e) => this.onVaultChanged(e.payload.paths));

    try {
      const [recent] = await api.recentVaults();
      if (recent) await this.openVault(recent.path);
    } catch {
      /* fall back to the welcome screen */
    }
    this.ready = true;
  }

  // ---------- appearance & settings ----------

  applyAppearance() {
    const { theme, accent, fontSize, lineWidth } = this.settings;
    this.dark = theme === "dark" || (theme === "system" && this.media.matches);
    const root = document.documentElement;
    root.dataset.appearance = this.dark ? "dark" : "light";
    root.dataset.theme = theme;
    root.dataset.accent = accent;
    root.style.setProperty("--editor-font-size", `${fontSize}px`);
    // ~0.55em is an average glyph width for the system sans, so the limit
    // stays the same number of characters as the text size changes.
    root.style.setProperty("--line-width", `${Math.round(lineWidth * fontSize * 0.55)}px`);
  }

  async updateSettings(patch: Partial<Settings>) {
    this.settings = { ...this.settings, ...patch };
    this.applyAppearance();
    if ("autoSyncMinutes" in patch) this.scheduleAutoSync();
    try {
      await api.saveSettings($state.snapshot(this.settings));
    } catch (e) {
      this.toast(`Could not save settings: ${message(e)}`, "error");
    }
  }

  // ---------- vault lifecycle ----------

  /**
   * Closes the current vault and opens another. Ignored while a switch is
   * already running, so repeated clicks can't interleave two opens.
   */
  private async switchVault(label: string, open: () => Promise<VaultInfo>, syncNow = true): Promise<boolean> {
    if (this.opening) return false;
    this.opening = label;
    try {
      await this.closeVault();
      await this.afterOpen(await open(), syncNow);
      return true;
    } finally {
      this.opening = null;
    }
  }

  async openVault(path: string) {
    await this.switchVault(fileName(path), () => api.openVault(path));
  }

  async pickAndOpenVault() {
    const dir = await openDialog({ directory: true, title: "Open folder as vault" });
    if (typeof dir !== "string") return;
    try {
      await this.openVault(dir);
    } catch (e) {
      this.toast(message(e), "error");
    }
  }

  async createVault(parent: string, name: string) {
    if (await this.switchVault(name, () => api.createVault(parent, name))) this.openFile("Welcome.md");
  }

  async cloneVault(repo: GithubRepo, destParent: string) {
    if (await this.switchVault(repo.name, () => api.gitCloneVault(repo, destParent), false)) {
      this.sync.lastAt = Date.now();
    }
  }

  async pullVault(folderId: string, folderName: string, destParent: string) {
    if (await this.switchVault(folderName, () => api.pullVault(folderId, folderName, destParent), false)) {
      this.sync.lastAt = Date.now();
    }
  }

  private async afterOpen(info: VaultInfo, syncNow = true) {
    this.vault = info;
    this.ignore = await api.gitIgnoreRules().catch(() => ({ extensions: [], paths: [] }));
    this.sync = { running: false, lastAt: null, error: null };
    await this.refresh();
    this.restoreWorkspace();
    this.scheduleAutoSync();
    if (syncNow && info.sync) void this.syncNow();
  }

  async closeVault() {
    if (!this.vault) return;
    await this.flushAll();
    this.saveWorkspace();
    clearInterval(this.syncTimer);
    await api.closeVault();
    this.vault = null;
    this.tabs = [];
    this.activeId = null;
    this.tree = [];
    this.notes = [];
    this.tags = [];
    this.expanded = {};
    this.selected = null;
  }

  async refresh() {
    if (!this.vault) return;
    const [tree, notes, tags] = await Promise.all([api.tree(), api.noteIndex(), api.tags()]);
    this.tree = tree;
    this.notes = notes;
    this.tags = tags;
  }

  private refreshSoon = debounce(() => void this.refresh().catch(() => {}), 150);

  private onVaultChanged(paths: string[]) {
    paths.forEach(invalidateBlobUrl);
    this.refreshSoon();
    for (const listener of this.fileListeners) listener(paths);
  }

  /** Subscribe to on-disk changes; returns an unsubscribe function. */
  onFilesChanged(listener: (paths: string[]) => void) {
    this.fileListeners.add(listener);
    return () => this.fileListeners.delete(listener);
  }

  /** Editors register a flush so pending edits are saved before closing. */
  registerFlusher(flush: () => Promise<void> | void) {
    this.flushers.add(flush);
    return () => this.flushers.delete(flush);
  }

  async flushAll() {
    await Promise.all([...this.flushers].map((f) => f()));
  }

  // ---------- workspace persistence (per vault, per device) ----------

  private workspaceKey() {
    return this.vault ? `asteroid:workspace:${this.vault.path}` : null;
  }

  saveWorkspace() {
    const key = this.workspaceKey();
    if (!key) return;
    const ws: Workspace = {
      tabs: this.tabs.map((t) => ({ path: t.path, mode: t.mode })),
      active: this.tabs.findIndex((t) => t.id === this.activeId),
      expanded: $state.snapshot(this.expanded),
      sidebarOpen: this.sidebarOpen,
      rightOpen: this.rightOpen,
      pdfOutlineOpen: this.pdfOutlineOpen,
    };
    try {
      localStorage.setItem(key, JSON.stringify(ws));
    } catch {
      /* storage unavailable */
    }
  }

  private restoreWorkspace() {
    const key = this.workspaceKey();
    let ws: Workspace | null = null;
    try {
      ws = key ? JSON.parse(localStorage.getItem(key) ?? "null") : null;
    } catch {
      ws = null;
    }
    const exists = new Set(this.notes.map((n) => n.path));
    if (ws) {
      this.expanded = ws.expanded ?? {};
      this.sidebarOpen = ws.sidebarOpen ?? true;
      this.rightOpen = ws.rightOpen ?? false;
      this.pdfOutlineOpen = ws.pdfOutlineOpen ?? false;
      this.tabs = ws.tabs
        .filter((t) => exists.has(t.path))
        .map((t) => this.makeTab(t.path, t.mode));
      const active = this.tabs[ws.active] ?? this.tabs[0];
      this.activeId = active?.id ?? null;
    }
    if (this.tabs.length === 0) {
      const first = this.notes.find((n) => n.kind === "note");
      if (first) this.openFile(first.path);
    }
  }

  // ---------- tabs & navigation ----------

  private makeTab(path: string, mode?: ViewMode): Tab {
    return {
      id: uid(),
      path,
      kind: kindOf(path),
      mode: mode ?? this.settings.defaultMode,
      history: [path],
      historyIndex: 0,
    };
  }

  openFile(path: string, opts: { newTab?: boolean; line?: number; heading?: string } = {}) {
    const reveal =
      opts.line !== undefined || opts.heading ? { line: opts.line, heading: opts.heading, nonce: Date.now() } : undefined;
    const existing = this.tabs.find((t) => t.path === path);
    if (existing) {
      this.activeId = existing.id;
      if (reveal) existing.reveal = reveal;
    } else if (opts.newTab || !this.activeTab) {
      const tab = this.makeTab(path);
      tab.reveal = reveal;
      this.tabs.push(tab);
      this.activeId = tab.id;
    } else {
      const tab = this.activeTab;
      tab.history = [...tab.history.slice(0, tab.historyIndex + 1), path];
      tab.historyIndex = tab.history.length - 1;
      this.navigateTab(tab, path);
      tab.reveal = reveal;
    }
    this.selected = path;
    this.expandTo(path);
    this.saveWorkspace();
  }

  private navigateTab(tab: Tab, path: string) {
    tab.path = path;
    tab.kind = kindOf(path);
    if (tab.kind === "note" && tab.mode !== this.settings.defaultMode) tab.mode = this.settings.defaultMode;
  }

  canGoBack = $derived(!!this.activeTab && this.activeTab.historyIndex > 0);
  canGoForward = $derived(!!this.activeTab && this.activeTab.historyIndex < this.activeTab.history.length - 1);

  back() {
    const tab = this.activeTab;
    if (!tab || tab.historyIndex === 0) return;
    tab.historyIndex -= 1;
    this.navigateTab(tab, tab.history[tab.historyIndex]);
    this.selected = tab.path;
  }

  forward() {
    const tab = this.activeTab;
    if (!tab || tab.historyIndex >= tab.history.length - 1) return;
    tab.historyIndex += 1;
    this.navigateTab(tab, tab.history[tab.historyIndex]);
    this.selected = tab.path;
  }

  closeTab(id: string) {
    const index = this.tabs.findIndex((t) => t.id === id);
    if (index < 0) return;
    this.tabs.splice(index, 1);
    if (this.activeId === id) {
      const next = this.tabs[index] ?? this.tabs[index - 1];
      this.activeId = next?.id ?? null;
      this.selected = next?.path ?? null;
    }
    this.saveWorkspace();
  }

  cycleTab(step: number) {
    if (this.tabs.length < 2) return;
    const index = this.tabs.findIndex((t) => t.id === this.activeId);
    const next = this.tabs[(index + step + this.tabs.length) % this.tabs.length];
    this.activeId = next.id;
    this.selected = next.path;
  }

  setMode(mode: ViewMode) {
    if (this.activeTab?.kind === "note") this.activeTab.mode = mode;
    this.saveWorkspace();
  }

  toggleMode() {
    const tab = this.activeTab;
    if (tab?.kind === "note") this.setMode(tab.mode === "edit" ? "read" : "edit");
  }

  expandTo(path: string) {
    let folder = parentOf(path);
    while (folder) {
      this.expanded[folder] = true;
      folder = parentOf(folder);
    }
  }

  /**
   * Follows a link from the note at `from`: URLs open in the browser,
   * `Note#Heading` targets resolve through the index, and unresolved links
   * create the note (as Obsidian does).
   */
  async followLink(target: string, from: string, opts: { newTab?: boolean; external?: boolean } = {}) {
    try {
      if (opts.external || /^[a-z][a-z0-9+.-]*:/i.test(target)) {
        await openUrl(target);
        return;
      }
      let ref = target;
      let heading: string | undefined;
      const hash = ref.indexOf("#");
      if (hash >= 0) {
        heading = ref.slice(hash + 1) || undefined;
        ref = ref.slice(0, hash);
      }
      try {
        ref = decodeURIComponent(ref);
      } catch {
        /* keep as written */
      }
      if (!ref) {
        if (heading) this.openFile(from, { heading });
        return;
      }
      const resolved = await api.resolveLink(ref, from);
      if (resolved) this.openFile(resolved, { newTab: opts.newTab, heading });
      else await this.newNote(parentOf(ref), fileName(ref).replace(/\.md$/i, ""));
    } catch (e) {
      this.toast(message(e), "error");
    }
  }

  // ---------- file operations ----------

  /** Folder that new notes go into: the selected folder, else the root. */
  targetFolder(): string {
    const sel = this.selected;
    if (!sel) return "";
    const isFolder = (entries: Entry[]): boolean =>
      entries.some((e) => (e.path === sel && e.kind === "folder") || (e.children ? isFolder(e.children) : false));
    return isFolder(this.tree) ? sel : "";
  }

  async newNote(folder = this.targetFolder(), name?: string) {
    try {
      const path = await api.createNote(folder, name);
      await this.refresh();
      this.openFile(path);
      if (!name && this.activeTab) {
        this.focusTitle.add(this.activeTab.id);
        this.activeTab.mode = "edit";
      }
      return path;
    } catch (e) {
      this.toast(message(e), "error");
    }
  }

  async newFolder(parent = this.targetFolder()) {
    const taken = new Set<string>();
    const collect = (entries: Entry[]) =>
      entries.forEach((e) => {
        taken.add(e.path);
        if (e.children) collect(e.children);
      });
    collect(this.tree);
    let name = "Untitled folder";
    for (let n = 2; taken.has(parent ? `${parent}/${name}` : name); n++) name = `Untitled folder ${n}`;
    try {
      const path = await api.createFolder(parent, name);
      if (parent) this.expanded[parent] = true;
      await this.refresh();
      this.selected = path;
      this.renaming = path;
    } catch (e) {
      this.toast(message(e), "error");
    }
  }

  /** Re-points tabs, selection and expansion after a rename or move. */
  private repath(from: string, to: string) {
    const map = (p: string) => (p === from ? to : p.startsWith(`${from}/`) ? to + p.slice(from.length) : p);
    for (const tab of this.tabs) {
      tab.path = map(tab.path);
      tab.kind = kindOf(tab.path);
      tab.history = tab.history.map(map);
    }
    if (this.selected) this.selected = map(this.selected);
    const expanded: Record<string, boolean> = {};
    for (const [k, v] of Object.entries(this.expanded)) expanded[map(k)] = v;
    this.expanded = expanded;
    invalidateBlobUrl(from);
  }

  async rename(path: string, newName: string) {
    const name = newName.trim();
    if (!name) return path;
    await this.flushAll();
    try {
      const { path: to, updatedLinks } = await api.rename(path, name, this.settings.updateLinks);
      if (to !== path) this.repath(path, to);
      await this.refresh();
      this.saveWorkspace();
      this.reportLinks(updatedLinks);
      return to;
    } catch (e) {
      this.toast(message(e), "error");
      return path;
    }
  }

  private reportLinks(updated: string[]) {
    if (!updated.length) return;
    const n = updated.length;
    this.toast(`Updated links in ${n} note${n === 1 ? "" : "s"}`, "success");
  }

  async move(path: string, newParent: string) {
    if (parentOf(path) === newParent) return;
    await this.flushAll();
    try {
      const { path: to, updatedLinks } = await api.move(path, newParent, this.settings.updateLinks);
      this.repath(path, to);
      this.reportLinks(updatedLinks);
      if (newParent) this.expanded[newParent] = true;
      await this.refresh();
      this.saveWorkspace();
    } catch (e) {
      this.toast(message(e), "error");
    }
  }

  async remove(path: string) {
    if (this.settings.confirmDelete) {
      const ok = await ask(`“${fileName(path)}” will be moved to the vault’s .trash folder.`, {
        title: "Delete?",
        kind: "warning",
        okLabel: "Delete",
      });
      if (!ok) return;
    }
    try {
      await api.delete(path);
      for (const tab of [...this.tabs]) {
        if (tab.path === path || tab.path.startsWith(`${path}/`)) this.closeTab(tab.id);
      }
      await this.refresh();
    } catch (e) {
      this.toast(message(e), "error");
    }
  }

  async reveal(path: string) {
    try {
      await api.reveal(path);
    } catch (e) {
      this.toast(message(e), "error");
    }
  }

  async importFiles(paths: string[], folder = this.targetFolder()) {
    try {
      const imported = await api.importFiles(paths, folder);
      await this.refresh();
      if (imported.length === 1) this.openFile(imported[0]);
      else if (imported.length > 1) this.toast(`Imported ${imported.length} files`, "success");
    } catch (e) {
      this.toast(message(e), "error");
    }
  }

  // ---------- sync ----------

  scheduleAutoSync() {
    clearInterval(this.syncTimer);
    const minutes = this.settings.autoSyncMinutes;
    if (!this.vault?.sync || minutes <= 0) return;
    this.syncTimer = setInterval(() => void this.syncNow(), minutes * 60_000);
  }

  /** Called after local edits; syncs once typing has settled. */
  syncSoon = debounce(() => {
    if (this.vault?.sync && this.settings.autoSyncMinutes > 0) void this.syncNow();
  }, 20_000);

  async syncNow(allowMassDelete = false) {
    if (!this.vault?.sync || this.sync.running) return;
    this.sync.running = true;
    this.sync.error = null;
    try {
      await this.flushAll();
      const report = await api.syncNow(allowMassDelete);
      this.sync.lastAt = Date.now();
      this.reportSync(report);
      await this.refresh();
    } catch (e) {
      const text = message(e);
      this.sync.error = text;
      if (text.startsWith("Sync stopped")) {
        this.toast(text, "error", { label: "Sync anyway", run: () => void this.syncNow(true) });
      }
    } finally {
      this.sync.running = false;
    }
  }

  private reportSync(r: SyncReport) {
    if (r.conflicts.length) {
      this.toast(
        `${r.conflicts.length} conflicting edit${r.conflicts.length > 1 ? "s" : ""} kept as a separate copy`,
        "info",
        { label: "Show", run: () => this.openFile(r.conflicts[0]) },
      );
    }
    if (r.errors.length) this.toast(`${r.errors.length} file(s) failed to sync: ${r.errors[0]}`, "error");
    if (r.skipped?.length) {
      const first = r.skipped[0].split("/").pop();
      const more = r.skipped.length > 1 ? ` and ${r.skipped.length - 1} more` : "";
      this.toast(`“${first}”${more} is over GitHub’s 100 MB limit and wasn’t synced`, "error", {
        label: "Details",
        run: () => {
          this.settingsSection = "sync";
          this.modal = "settings";
        },
      });
    }
  }

  /** The vault syncs with git (or already is a repository). */
  get usesGit() {
    return !!this.vault && (this.vault.hasHistory || this.vault.sync?.provider === "git");
  }

  /** Whether `path` (a file, or a folder when `folder`) is excluded from git sync. */
  isExcluded(path: string, folder = false): boolean {
    const { extensions, paths } = this.ignore;
    const ext = path.includes(".") ? path.slice(path.lastIndexOf(".") + 1).toLowerCase() : "";
    if (!folder && ext && extensions.includes(ext)) return true;
    return paths.some((p) => (p.endsWith("/") ? `${path}/`.startsWith(p) : p === path));
  }

  async setIgnoreRules(rules: IgnoreRules) {
    try {
      this.ignore = await api.gitSetIgnoreRules(rules);
      this.ignoreVersion++;
      this.syncSoon();
    } catch (e) {
      this.toast(message(e), "error");
    }
  }

  /** Adds or removes one file or folder from the "don't sync" list. */
  async toggleExcluded(path: string, folder: boolean) {
    const rule = folder ? `${path}/` : path;
    const paths = this.ignore.paths.includes(rule) ? this.ignore.paths.filter((p) => p !== rule) : [...this.ignore.paths, rule];
    await this.setIgnoreRules({ ...this.ignore, paths });
  }

  /** Opens version history for `path`, or recently deleted files when null. */
  showHistory(path: string | null) {
    this.historyPath = path;
    this.modal = "history";
  }

  setVaultSync(link: VaultInfo["sync"]) {
    if (!this.vault) return;
    this.vault.sync = link;
    if (link?.provider === "git") this.vault.hasHistory = true;
    this.scheduleAutoSync();
  }

  // ---------- toasts ----------

  toast(text: string, kind: Toast["kind"] = "info", action?: Toast["action"]) {
    const toast: Toast = { id: uid(), text, kind, action };
    this.toasts.push(toast);
    setTimeout(() => this.dismissToast(toast.id), action ? 9000 : 4500);
  }

  dismissToast(id: string) {
    const i = this.toasts.findIndex((t) => t.id === id);
    if (i >= 0) this.toasts.splice(i, 1);
  }
}

export const app = new AppState();
