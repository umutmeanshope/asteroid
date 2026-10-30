// Sets the app version everywhere it's declared, so releases stay in sync:
//   node scripts/bump-version.mjs 0.2.0
import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version ?? "")) {
  console.error("Usage: node scripts/bump-version.mjs <major.minor.patch>");
  process.exit(1);
}

const edit = (file, from, to) => {
  const text = readFileSync(file, "utf8");
  if (!from.test(text)) throw new Error(`No version found in ${file}`);
  writeFileSync(file, text.replace(from, to));
  console.log(`${file} → ${version}`);
};

edit("package.json", /("version":\s*")[^"]+(")/, `$1${version}$2`);
edit("src-tauri/tauri.conf.json", /("version":\s*")[^"]+(")/, `$1${version}$2`);
// The [workspace.package] version, shared by every crate.
edit("Cargo.toml", /(\[workspace\.package\][^[]*?\nversion = ")[^"]+(")/, `$1${version}$2`);
console.log("Run `cargo check` (or any build) to refresh Cargo.lock, then commit.");
