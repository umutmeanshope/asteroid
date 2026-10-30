// Mirrors of the Rust types that cross the IPC boundary.

export type EntryKind = "folder" | "note" | "pdf" | "image" | "other";

export interface Entry {
  name: string;
  path: string;
  kind: EntryKind;
  children?: Entry[];
}

/** Result of a rename or move: the new path and notes whose links followed. */
export interface MoveResult {
  path: string;
  updatedLinks: string[];
}

export type SyncLink =
  | { provider: "gdrive"; folderId: string; folderName: string }
  | { provider: "git"; remoteUrl: string; repoName: string; branch: string };

export interface VaultInfo {
  name: string;
  path: string;
  sync: SyncLink | null;
  /** The vault is a git repository, so version history is available. */
  hasHistory: boolean;
}

/** One commit that changed a file. */
export interface Version {
  commit: string;
  timeMs: number;
  author: string;
  message: string;
  kind: "added" | "modified" | "deleted";
  additions: number | null;
  deletions: number | null;
}

export interface DeletedFile {
  path: string;
  deletedIn: string;
  restoreFrom: string;
  timeMs: number;
  author: string;
}

export interface DiffLine {
  kind: "+" | "-" | " ";
  text: string;
  oldLine: number | null;
  newLine: number | null;
}

export interface FileDiff {
  binary: boolean;
  hunks: { oldStart: number; newStart: number; lines: DiffLine[] }[];
  additions: number;
  deletions: number;
  truncated: boolean;
}

export interface RecentVault {
  path: string;
  name: string;
  openedMs: number;
}

export interface NoteSummary {
  path: string;
  title: string;
  kind: EntryKind;
  aliases?: string[];
}

export interface SearchHit {
  path: string;
  title: string;
  score: number;
  line: number | null;
  snippet: string;
}

export interface Backlink {
  source: string;
  title: string;
  line: number;
  context: string;
}

export interface Heading {
  level: number;
  text: string;
  line: number;
}

export interface TagCount {
  tag: string;
  count: number;
}

export interface DriveStatus {
  configured: boolean;
  connected: boolean;
  email: string | null;
}

export interface GithubStatus {
  configured: boolean;
  connected: boolean;
  login: string | null;
}

/** A pending GitHub sign-in: the user enters `userCode` at `verificationUri`. */
export interface DeviceCode {
  deviceCode: string;
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
}

export interface GithubRepo {
  fullName: string;
  name: string;
  private: boolean;
  cloneUrl: string;
  defaultBranch: string;
  description: string | null;
  updatedAt: string | null;
}

export interface DriveFolder {
  id: string;
  name: string;
}

export interface SyncReport {
  uploaded: string[];
  downloaded: string[];
  deleted_local: string[];
  deleted_remote: string[];
  conflicts: string[];
  errors: string[];
  /** Files deliberately left out, e.g. too large for GitHub. */
  skipped: string[];
}

export interface SizedFile {
  path: string;
  bytes: number;
}

/** A vault measured against GitHub's limits (sizes in bytes). */
export interface SizeReport {
  fileCount: number;
  totalBytes: number;
  historyBytes: number;
  largeFiles: SizedFile[];
  warnFileBytes: number;
  maxFileBytes: number;
  recommendedRepoBytes: number;
  /** Files left out by .gitignore. */
  ignoredCount: number;
  ignoredBytes: number;
  /** Every visible file grouped by lower-case extension, largest first. */
  byExtension: { ext: string; count: number; bytes: number }[];
}

/** Asteroid's "don't sync" rules in the vault's .gitignore. */
export interface IgnoreRules {
  /** Lower-case extensions without the dot. */
  extensions: string[];
  /** Vault-relative paths; folders end with "/". */
  paths: string[];
}

export type Theme = "system" | "light" | "dark";
export type Accent = "blue" | "purple" | "pink" | "red" | "orange" | "yellow" | "green" | "graphite";

export interface Settings {
  theme: Theme;
  accent: Accent;
  fontSize: number;
  readableWidth: boolean;
  /** Rewrite links across the vault when a file is renamed or moved. */
  updateLinks: boolean;
  /** Approximate characters per line when readableWidth is on. */
  lineWidth: number;
  livePreview: boolean;
  spellcheck: boolean;
  defaultMode: "edit" | "read";
  confirmDelete: boolean;
  autoSyncMinutes: number;
  googleClientId: string;
  googleClientSecret: string;
  githubClientId: string;
}

export type ViewMode = "edit" | "read";

export interface Tab {
  id: string;
  path: string;
  kind: EntryKind;
  mode: ViewMode;
  history: string[];
  historyIndex: number;
  /** One-shot request to scroll to a line or heading once the file is shown. */
  reveal?: { line?: number; heading?: string; nonce: number };
}
