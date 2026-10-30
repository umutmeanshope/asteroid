// Plain-text clipboard through Tauri: WebKitGTK's navigator.clipboard can't
// reliably read, which the editor's Paste menu item needs.

import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";

export async function copyText(text: string) {
  await writeText(text);
}

export async function pasteText(): Promise<string> {
  return (await readText()) ?? "";
}
