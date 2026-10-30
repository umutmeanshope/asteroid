use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use walkdir::WalkDir;

use crate::{Error, Result};

/// Folder inside every vault that holds Asteroid's own metadata.
pub const META_DIR: &str = ".asteroid";
/// Metadata folder name used before the app was renamed; migrated on open.
const LEGACY_META_DIR: &str = ".tachylite";
/// Folder that receives deleted files instead of removing them outright.
pub const TRASH_DIR: &str = ".trash";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Folder,
    Note,
    Pdf,
    Image,
    Other,
}

impl EntryKind {
    pub fn from_path(path: &str) -> Self {
        match extension(path).as_deref() {
            Some("md") | Some("markdown") => EntryKind::Note,
            Some("pdf") => EntryKind::Pdf,
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "avif" | "ico") => {
                EntryKind::Image
            }
            _ => EntryKind::Other,
        }
    }
}

/// A node of the vault's file tree.
#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub name: String,
    /// Vault-relative path using `/` separators.
    pub path: String,
    pub kind: EntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Entry>>,
}

/// Flat file metadata used by the index and the sync engine.
#[derive(Debug, Clone, Serialize)]
pub struct FileInfo {
    pub path: String,
    pub kind: EntryKind,
    pub size: u64,
    /// Modification time in milliseconds since the Unix epoch.
    pub modified_ms: i64,
}

#[derive(Debug, Clone)]
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    /// Opens an existing folder as a vault.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        if !root.is_dir() {
            return Err(Error::NotAVault(root.display().to_string()));
        }
        let root = fs::canonicalize(root)?;
        let legacy = root.join(LEGACY_META_DIR);
        if legacy.is_dir() && !root.join(META_DIR).exists() {
            fs::rename(&legacy, root.join(META_DIR))?;
        }
        fs::create_dir_all(root.join(META_DIR))?;
        Ok(Self { root })
    }

    /// Creates `parent/name` as a new, empty vault with a welcome note.
    pub fn create(parent: impl AsRef<Path>, name: &str) -> Result<Self> {
        validate_name(name)?;
        let root = parent.as_ref().join(name);
        if root.exists() {
            return Err(Error::AlreadyExists(root.display().to_string()));
        }
        fs::create_dir_all(&root)?;
        let vault = Self::open(&root)?;
        vault.write_text("Welcome.md", WELCOME_NOTE)?;
        Ok(vault)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn name(&self) -> String {
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Vault".into())
    }

    pub fn meta_dir(&self) -> PathBuf {
        self.root.join(META_DIR)
    }

    /// Resolves a vault-relative path to an absolute one, rejecting anything
    /// that would escape the vault root.
    pub fn resolve(&self, rel: &str) -> Result<PathBuf> {
        let rel = rel.replace('\\', "/");
        let rel = rel.trim_start_matches('/');
        let mut out = self.root.clone();
        for component in Path::new(rel).components() {
            match component {
                Component::Normal(part) => out.push(part),
                Component::CurDir => {}
                _ => return Err(Error::OutsideVault(rel.to_string())),
            }
        }
        Ok(out)
    }

    /// Converts an absolute path inside the vault back to a relative one.
    pub fn relative(&self, abs: &Path) -> Option<String> {
        let rel = abs.strip_prefix(&self.root).ok()?;
        let parts: Vec<_> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        Some(parts.join("/"))
    }

    /// Whether a vault-relative path lives in a hidden or metadata folder.
    pub fn is_hidden(rel: &str) -> bool {
        rel.split('/').any(|part| part.starts_with('.'))
    }

    /// Returns the nested file tree, folders first, then alphabetical.
    pub fn tree(&self) -> Result<Vec<Entry>> {
        self.read_dir_tree(&self.root)
    }

    fn read_dir_tree(&self, dir: &Path) -> Result<Vec<Entry>> {
        let mut entries = Vec::new();
        for item in fs::read_dir(dir)? {
            let item = item?;
            let name = item.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = item.path();
            let Some(rel) = self.relative(&path) else { continue };
            let file_type = item.file_type()?;
            if file_type.is_dir() {
                entries.push(Entry {
                    name,
                    path: rel,
                    kind: EntryKind::Folder,
                    children: Some(self.read_dir_tree(&path)?),
                });
            } else if file_type.is_file() {
                let kind = EntryKind::from_path(&rel);
                entries.push(Entry { name, path: rel, kind, children: None });
            }
        }
        entries.sort_by(|a, b| {
            let a_folder = a.kind == EntryKind::Folder;
            let b_folder = b.kind == EntryKind::Folder;
            b_folder
                .cmp(&a_folder)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    }

    /// Every visible file in the vault, recursively.
    pub fn files(&self) -> Result<Vec<FileInfo>> {
        let mut out = Vec::new();
        let walker = WalkDir::new(&self.root)
            .min_depth(1)
            .into_iter()
            .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'));
        for entry in walker {
            let entry = entry.map_err(|e| Error::Io(e.into()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let Some(rel) = self.relative(entry.path()) else { continue };
            let meta = entry.metadata().map_err(|e| Error::Io(e.into()))?;
            out.push(FileInfo {
                kind: EntryKind::from_path(&rel),
                path: rel,
                size: meta.len(),
                modified_ms: modified_ms(&meta),
            });
        }
        Ok(out)
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.resolve(rel).map(|p| p.exists()).unwrap_or(false)
    }

    pub fn read_text(&self, rel: &str) -> Result<String> {
        let path = self.existing(rel)?;
        Ok(fs::read_to_string(path)?)
    }

    pub fn read_bytes(&self, rel: &str) -> Result<Vec<u8>> {
        let path = self.existing(rel)?;
        Ok(fs::read(path)?)
    }

    pub fn write_text(&self, rel: &str, content: &str) -> Result<()> {
        self.write_bytes(rel, content.as_bytes())
    }

    /// Writes atomically: the data lands in a sibling temp file first and is
    /// then renamed over the destination, so a crash never leaves a torn note.
    pub fn write_bytes(&self, rel: &str, data: &[u8]) -> Result<()> {
        let path = self.resolve(rel)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file_name = path
            .file_name()
            .ok_or_else(|| Error::InvalidName(rel.to_string()))?
            .to_string_lossy();
        let tmp = path.with_file_name(format!(".{file_name}.tmp"));
        {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(data)?;
            file.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Creates a new note in `folder`, picking a free name based on `name`.
    pub fn create_note(&self, folder: &str, name: Option<&str>) -> Result<String> {
        let base = name.map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Untitled");
        let base = base.strip_suffix(".md").unwrap_or(base);
        validate_name(base)?;
        let rel = self.free_path(folder, base, Some("md"))?;
        self.write_text(&rel, "")?;
        Ok(rel)
    }

    pub fn create_folder(&self, parent: &str, name: &str) -> Result<String> {
        validate_name(name)?;
        let rel = join(parent, name);
        let path = self.resolve(&rel)?;
        if path.exists() {
            return Err(Error::AlreadyExists(rel));
        }
        fs::create_dir_all(path)?;
        Ok(rel)
    }

    /// Renames a file or folder in place. Returns the new relative path.
    pub fn rename(&self, rel: &str, new_name: &str) -> Result<String> {
        validate_name(new_name)?;
        let from = self.existing(rel)?;
        let new_rel = join(parent_of(rel), new_name);
        let to = self.resolve(&new_rel)?;
        if to.exists() && !same_file_ignoring_case(&from, &to) {
            return Err(Error::AlreadyExists(new_rel));
        }
        fs::rename(from, to)?;
        Ok(new_rel)
    }

    /// Moves a file or folder into `new_parent` (`""` is the vault root).
    pub fn move_entry(&self, rel: &str, new_parent: &str) -> Result<String> {
        let from = self.existing(rel)?;
        let name = file_name_of(rel);
        let new_rel = join(new_parent, name);
        if new_rel == rel {
            return Ok(new_rel);
        }
        if new_rel.starts_with(&format!("{rel}/")) {
            return Err(Error::InvalidName("cannot move a folder into itself".into()));
        }
        let to = self.resolve(&new_rel)?;
        if to.exists() {
            return Err(Error::AlreadyExists(new_rel));
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(from, to)?;
        Ok(new_rel)
    }

    /// Moves a file or folder into the vault's `.trash` folder.
    pub fn trash(&self, rel: &str) -> Result<()> {
        let from = self.existing(rel)?;
        let stem_path = Path::new(file_name_of(rel));
        let stem = stem_path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let ext = stem_path.extension().map(|e| e.to_string_lossy().into_owned());
        let parent = join(TRASH_DIR, parent_of(rel));
        let dest_rel = self.free_path(&parent, &stem, ext.as_deref())?;
        let dest = self.resolve(&dest_rel)?;
        if let Some(dir) = dest.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::rename(from, dest)?;
        Ok(())
    }

    /// Finds `folder/base.ext`, `folder/base 1.ext`, ... that does not exist yet.
    pub fn free_path(&self, folder: &str, base: &str, ext: Option<&str>) -> Result<String> {
        let file = |n: usize| {
            let suffix = if n == 0 { String::new() } else { format!(" {n}") };
            match ext {
                Some(ext) => format!("{base}{suffix}.{ext}"),
                None => format!("{base}{suffix}"),
            }
        };
        for n in 0..10_000 {
            let rel = join(folder, &file(n));
            if !self.resolve(&rel)?.exists() {
                return Ok(rel);
            }
        }
        Err(Error::AlreadyExists(join(folder, base)))
    }

    fn existing(&self, rel: &str) -> Result<PathBuf> {
        let path = self.resolve(rel)?;
        if !path.exists() {
            return Err(Error::NotFound(rel.to_string()));
        }
        Ok(path)
    }
}

pub fn modified_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Joins two vault-relative paths with `/`.
pub fn join(parent: &str, name: &str) -> String {
    let parent = parent.trim_matches('/');
    if parent.is_empty() { name.to_string() } else { format!("{parent}/{name}") }
}

pub fn parent_of(rel: &str) -> &str {
    rel.rfind('/').map(|i| &rel[..i]).unwrap_or("")
}

pub fn file_name_of(rel: &str) -> &str {
    rel.rfind('/').map(|i| &rel[i + 1..]).unwrap_or(rel)
}

/// File name without its extension, used as a note's title.
pub fn stem_of(rel: &str) -> &str {
    let name = file_name_of(rel);
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

pub fn extension(rel: &str) -> Option<String> {
    let name = file_name_of(rel);
    let i = name.rfind('.')?;
    (i > 0).then(|| name[i + 1..].to_lowercase())
}

fn validate_name(name: &str) -> Result<()> {
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.starts_with('.')
        || name.chars().any(|c| matches!(c, '/' | '\\' | '\0' | ':'));
    if bad { Err(Error::InvalidName(name.to_string())) } else { Ok(()) }
}

/// Allows case-only renames on case-insensitive filesystems.
fn same_file_ignoring_case(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

const WELCOME_NOTE: &str = "# Welcome to Asteroid\n\nThis is your new vault. Notes are plain Markdown files in this folder.\n\n- Link notes with `[[double brackets]]`, like [[Welcome]].\n- Tag them with #ideas or #todo.\n- Press **Ctrl/Cmd + O** to jump to any note.\n- Press **Ctrl/Cmd + E** to switch between editing and reading.\n\n> [!TIP]\n> Connect Google Drive in Settings to keep this vault in sync across devices.\n\n- [ ] Write your first note\n- [ ] Drop a PDF into the vault\n";

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> (tempfile::TempDir, Vault) {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        (dir, vault)
    }

    #[test]
    fn rejects_paths_outside_the_vault() {
        let (_dir, vault) = vault();
        assert!(vault.resolve("../etc/passwd").is_err());
        assert!(vault.resolve("a/../../b").is_err());
        assert!(vault.resolve("notes/a.md").is_ok());
    }

    #[test]
    fn migrates_legacy_metadata_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(LEGACY_META_DIR)).unwrap();
        fs::write(dir.path().join(LEGACY_META_DIR).join("sync.json"), "{}").unwrap();
        Vault::open(dir.path()).unwrap();
        assert!(dir.path().join(META_DIR).join("sync.json").is_file());
        assert!(!dir.path().join(LEGACY_META_DIR).exists());
    }

    #[test]
    fn creates_unique_note_names() {
        let (_dir, vault) = vault();
        assert_eq!(vault.create_note("", None).unwrap(), "Untitled.md");
        assert_eq!(vault.create_note("", None).unwrap(), "Untitled 1.md");
        vault.create_folder("", "Projects").unwrap();
        assert_eq!(vault.create_note("Projects", Some("Plan")).unwrap(), "Projects/Plan.md");
    }

    #[test]
    fn tree_hides_metadata_and_sorts_folders_first() {
        let (_dir, vault) = vault();
        vault.write_text("b.md", "").unwrap();
        vault.write_text("A/c.md", "").unwrap();
        let tree = vault.tree().unwrap();
        let names: Vec<_> = tree.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["A", "b.md"]);
    }

    #[test]
    fn rename_move_and_trash() {
        let (_dir, vault) = vault();
        vault.write_text("a.md", "x").unwrap();
        let renamed = vault.rename("a.md", "b.md").unwrap();
        assert_eq!(renamed, "b.md");
        let moved = vault.move_entry("b.md", "Archive").unwrap();
        assert_eq!(moved, "Archive/b.md");
        vault.trash("Archive/b.md").unwrap();
        assert!(!vault.exists("Archive/b.md"));
        assert!(vault.exists(".trash/Archive/b.md"));
        assert!(vault.files().unwrap().is_empty());
    }
}
