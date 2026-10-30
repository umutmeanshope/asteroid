// Copies the pdf.js runtime assets (wasm decoders, CMaps, fonts, ICC profiles) into
// public/ so they are served in dev and bundled into the app.
import { cpSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const src = join(root, "node_modules", "pdfjs-dist");
const dest = join(root, "public", "pdfjs");

for (const dir of ["wasm", "cmaps", "standard_fonts", "iccs"]) {
  const from = join(src, dir);
  if (!existsSync(from)) continue;
  mkdirSync(dest, { recursive: true });
  cpSync(from, join(dest, dir), { recursive: true });
}
