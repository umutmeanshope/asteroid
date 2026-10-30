import type { EntryKind } from "./types";

export const isMac = navigator.userAgent.includes("Mac");
export const modKey = isMac ? "⌘" : "Ctrl";

export function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

export function parentOf(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? "" : path.slice(0, i);
}

export function stem(path: string): string {
  const name = fileName(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

export function kindOf(path: string): EntryKind {
  const ext = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
  if (ext === "md" || ext === "markdown") return "note";
  if (ext === "pdf") return "pdf";
  if (["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "avif", "ico"].includes(ext)) return "image";
  return "other";
}

/** Display name: notes drop their extension, other files keep it. */
export function displayName(path: string): string {
  return kindOf(path) === "note" ? stem(path) : fileName(path);
}

export function debounce<A extends unknown[]>(fn: (...args: A) => void, ms: number) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending: A | undefined;
  const run = (...args: A) => {
    pending = args;
    clearTimeout(timer);
    timer = setTimeout(() => {
      const a = pending!;
      pending = undefined;
      fn(...a);
    }, ms);
  };
  run.flush = () => {
    if (!pending) return;
    clearTimeout(timer);
    const a = pending;
    pending = undefined;
    fn(...a);
  };
  run.cancel = () => {
    clearTimeout(timer);
    pending = undefined;
  };
  return run;
}

/**
 * Fuzzy match used by the quick switcher. Returns a score (higher is better)
 * or -1 when `query` is not a subsequence of `text`. Consecutive matches and
 * matches at word boundaries score higher.
 */
export function fuzzyScore(query: string, text: string): number {
  if (!query) return 0;
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (t === q) return 1000;
  if (t.startsWith(q)) return 500 - t.length;
  const contains = t.indexOf(q);
  if (contains >= 0) return 300 - contains - t.length * 0.1;
  let score = 0;
  let ti = 0;
  let streak = 0;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found < 0) return -1;
    const boundary = found === 0 || " /-_.".includes(t[found - 1]);
    streak = found === ti ? streak + 1 : 0;
    score += 10 + streak * 5 + (boundary ? 15 : 0) - (found - ti) * 0.5;
    ti = found + 1;
  }
  return score - t.length * 0.1;
}

export function relativeTime(ms: number): string {
  const s = Math.round((Date.now() - ms) / 1000);
  if (s < 10) return "just now";
  if (s < 60) return `${s}s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  return new Date(ms).toLocaleDateString();
}

let counter = 0;
export function uid(): string {
  counter += 1;
  return `${Date.now().toString(36)}-${counter}`;
}

/** Cached blob URLs for vault images so each file is read once. */
const blobUrls = new Map<string, Promise<string>>();

export function vaultBlobUrl(path: string, read: (p: string) => Promise<ArrayBuffer>): Promise<string> {
  let url = blobUrls.get(path);
  if (!url) {
    url = read(path).then((buf) => {
      const type = path.toLowerCase().endsWith(".svg") ? "image/svg+xml" : "";
      return URL.createObjectURL(new Blob([buf], type ? { type } : undefined));
    });
    url.catch(() => blobUrls.delete(path));
    blobUrls.set(path, url);
  }
  return url;
}

export function invalidateBlobUrl(path: string) {
  const url = blobUrls.get(path);
  if (url) {
    blobUrls.delete(path);
    url.then((u) => URL.revokeObjectURL(u)).catch(() => {});
  }
}

/** Display form of a CodeMirror-style shortcut such as "Mod-Shift-x": ⇧⌘X on macOS, Ctrl+Shift+X elsewhere. */
export function shortcutLabel(keys: string): string {
  const parts = keys.split("-");
  const key = parts.pop()!;
  const label = key === "Enter" ? (isMac ? "↩" : "Enter") : key.toUpperCase();
  const mods = parts.map((m) => (m === "Mod" ? (isMac ? "⌘" : "Ctrl") : m === "Shift" ? (isMac ? "⇧" : "Shift") : m));
  // macOS lists modifiers as Shift before Command.
  return isMac ? [...mods.reverse(), label].join("") : [...mods, label].join("+");
}

/** What a vault syncs with, for labels: a Drive folder or a repository name. */
export function syncTargetName(link: import("./types").SyncLink): string {
  return link.provider === "gdrive" ? link.folderName : link.repoName;
}

/** "526 MB", "1.2 GB": binary units, matching how GitHub states its limits. */
export function formatBytes(bytes: number): string {
  const units = ["bytes", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${unit >= 3 && value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}
