// Typed wrappers around the Tauri commands defined in src-tauri/src/commands.
import { invoke } from "@tauri-apps/api/core";
import type {
  Backlink,
  DriveFolder,
  DeletedFile,
  DeviceCode,
  DriveStatus,
  Entry,
  FileDiff,
  GithubRepo,
  GithubStatus,
  Heading,
  MoveResult,
  IgnoreRules,
  NoteSummary,
  RecentVault,
  SearchHit,
  SizeReport,
  SyncLink,
  SyncReport,
  TagCount,
  VaultInfo,
  Version,
} from "./types";

export const api = {
  recentVaults: () => invoke<RecentVault[]>("recent_vaults"),
  forgetRecentVault: (path: string) => invoke<void>("forget_recent_vault", { path }),
  openVault: (path: string) => invoke<VaultInfo>("open_vault", { path }),
  createVault: (parent: string, name: string) => invoke<VaultInfo>("create_vault", { parent, name }),
  closeVault: () => invoke<void>("close_vault"),

  tree: () => invoke<Entry[]>("vault_tree"),
  readNote: (path: string) => invoke<string>("read_note", { path }),
  writeNote: (path: string, content: string) => invoke<void>("write_note", { path, content }),
  readFile: (path: string) => invoke<ArrayBuffer>("read_file", { path }),
  createNote: (folder: string, name?: string) => invoke<string>("create_note", { folder, name }),
  createFolder: (parent: string, name: string) => invoke<string>("create_folder", { parent, name }),
  rename: (path: string, newName: string, updateLinks: boolean) =>
    invoke<MoveResult>("rename_entry", { path, newName, updateLinks }),
  move: (path: string, newParent: string, updateLinks: boolean) =>
    invoke<MoveResult>("move_entry", { path, newParent, updateLinks }),
  delete: (path: string) => invoke<void>("delete_entry", { path }),
  importFiles: (paths: string[], folder: string) => invoke<string[]>("import_files", { paths, folder }),
  reveal: (path: string) => invoke<void>("reveal_entry", { path }),

  noteIndex: () => invoke<NoteSummary[]>("note_index"),
  search: (query: string) => invoke<SearchHit[]>("search", { query }),
  backlinks: (path: string) => invoke<Backlink[]>("backlinks", { path }),
  outline: (content: string) => invoke<Heading[]>("outline", { content }),
  resolveLink: (target: string, from: string) => invoke<string | null>("resolve_link", { target, from }),
  renderEmbed: (target: string, from: string) => invoke<string | null>("render_embed", { target, from }),
  renderNote: (path: string, content: string) => invoke<string>("render_note", { path, content }),
  tags: () => invoke<TagCount[]>("tags"),

  loadSettings: () => invoke<Record<string, unknown>>("load_settings"),
  saveSettings: (settings: unknown) => invoke<void>("save_settings", { settings }),

  driveStatus: () => invoke<DriveStatus>("gdrive_status"),
  driveConnect: () => invoke<string>("gdrive_connect"),
  driveDisconnect: () => invoke<void>("gdrive_disconnect"),
  driveListFolders: (parent?: string) => invoke<DriveFolder[]>("gdrive_list_folders", { parent }),
  driveCreateFolder: (name: string, parent?: string) =>
    invoke<DriveFolder>("gdrive_create_folder", { name, parent }),
  syncLink: (folderId: string, folderName: string) => invoke<SyncLink>("sync_link", { folderId, folderName }),
  syncUnlink: () => invoke<void>("sync_unlink"),
  githubStatus: () => invoke<GithubStatus>("github_status"),
  githubDeviceStart: () => invoke<DeviceCode>("github_device_start"),
  githubDevicePoll: (code: DeviceCode) => invoke<string>("github_device_poll", { code }),
  githubDeviceCancel: () => invoke<void>("github_device_cancel"),
  githubDisconnect: () => invoke<void>("github_disconnect"),
  githubRepos: () => invoke<GithubRepo[]>("github_repos"),
  githubCreateRepo: (name: string, isPrivate: boolean) => invoke<GithubRepo>("github_create_repo", { name, private: isPrivate }),
  gitLink: (repo: GithubRepo) => invoke<SyncLink>("git_link", { repo }),
  gitSizeReport: () => invoke<SizeReport>("git_size_report"),
  gitIgnoreRules: () => invoke<IgnoreRules>("git_ignore_rules"),
  gitSetIgnoreRules: (rules: IgnoreRules) => invoke<IgnoreRules>("git_set_ignore_rules", { rules }),
  gitFileHistory: (path: string) => invoke<Version[]>("git_file_history", { path }),
  gitVersionDiff: (path: string, commit: string, against: "previous" | "working") =>
    invoke<FileDiff>("git_version_diff", { path, commit, against }),
  gitVersionText: (path: string, commit: string) => invoke<string>("git_version_text", { path, commit }),
  gitDeletedFiles: () => invoke<DeletedFile[]>("git_deleted_files"),
  gitRestoreVersion: (path: string, commit: string) => invoke<void>("git_restore_version", { path, commit }),
  gitCloneVault: (repo: GithubRepo, destParent: string) => invoke<VaultInfo>("git_clone_vault", { repo, destParent }),
  syncNow: (allowMassDelete = false) => invoke<SyncReport>("sync_now", { allowMassDelete }),
  pullVault: (folderId: string, folderName: string, destParent: string) =>
    invoke<VaultInfo>("pull_vault", { folderId, folderName, destParent }),
};

/** Normalises errors thrown by `invoke` (plain strings) into messages. */
export function message(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
