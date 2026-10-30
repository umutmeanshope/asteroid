//! Read-only views of a vault's git history: the versions of one file, what
//! each version changed, and files that were deleted. Restoring is left to
//! the caller, which writes the returned bytes through the vault.

use std::collections::HashSet;
use std::path::Path;

use git2::{Blob, Commit, DiffOptions, Oid, Patch, Repository, Sort};
use serde::Serialize;

use crate::{Error, Result};

/// How far back the deleted-files scan looks.
const DELETED_SCAN_COMMITS: usize = 3000;
/// Diffs longer than this are cut off (with `truncated` set).
const MAX_DIFF_LINES: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

/// One commit that changed a file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub commit: String,
    pub time_ms: i64,
    pub author: String,
    pub message: String,
    pub kind: ChangeKind,
    /// Line counts against the previous version; `None` for binary files.
    pub additions: Option<usize>,
    pub deletions: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedFile {
    pub path: String,
    /// The commit that deleted it, and the one that still has it.
    pub deleted_in: String,
    pub restore_from: String,
    pub time_ms: i64,
    pub author: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    /// '+', '-' or ' '.
    pub kind: char,
    pub text: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub binary: bool,
    pub hunks: Vec<Hunk>,
    pub additions: usize,
    pub deletions: usize,
    pub truncated: bool,
}

/// What a version is compared with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Against {
    /// The version before it: what that commit changed.
    Previous,
    /// The file as it is on disk now.
    Working,
}

/// Commits that changed `path`, newest first. A merge counts only when the
/// file differs from every parent, i.e. when the merge itself changed it.
pub fn file_history(root: &Path, path: &str, limit: usize) -> Result<Vec<Version>> {
    let repo = Repository::open(root)?;
    let Some(mut walk) = revwalk(&repo)? else { return Ok(Vec::new()) };
    walk.set_sorting(Sort::TIME)?;
    let file = Path::new(path);
    let mut versions = Vec::new();
    for oid in walk {
        let commit = repo.find_commit(oid?)?;
        let mine = blob_id(&commit, file);
        let parents: Vec<Option<Oid>> = commit.parents().map(|p| blob_id(&p, file)).collect();
        let changed = if parents.is_empty() { mine.is_some() } else { parents.iter().all(|p| *p != mine) };
        if !changed {
            continue;
        }
        let before = parents.first().copied().flatten();
        let kind = match (before, mine) {
            (None, _) => ChangeKind::Added,
            (Some(_), None) => ChangeKind::Deleted,
            _ => ChangeKind::Modified,
        };
        let (additions, deletions) = stats(&repo, before, mine, path)?;
        versions.push(Version {
            commit: oid_string(commit.id()),
            time_ms: commit.time().seconds() * 1000,
            author: commit.author().name().unwrap_or("Unknown").to_string(),
            message: commit.summary().ok().flatten().unwrap_or("").to_string(),
            kind,
            additions,
            deletions,
        });
        if versions.len() >= limit {
            break;
        }
    }
    Ok(versions)
}

/// The file's bytes at `commit` (a commit id or "HEAD").
pub fn read_version(root: &Path, commit: &str, path: &str) -> Result<Vec<u8>> {
    let repo = Repository::open(root)?;
    let commit = find_commit(&repo, commit)?;
    let id = blob_id(&commit, Path::new(path)).ok_or_else(|| Error::Setup(format!("{path} isn't in that version.")))?;
    Ok(repo.find_blob(id)?.content().to_vec())
}

/// A line diff of the file at `commit` against the previous version, or of
/// the file on disk (`working`, its current bytes) against `commit`.
pub fn diff_version(root: &Path, commit: &str, path: &str, against: Against, working: Option<&[u8]>) -> Result<FileDiff> {
    let repo = Repository::open(root)?;
    let commit = find_commit(&repo, commit)?;
    let file = Path::new(path);
    let at = blob(&repo, blob_id(&commit, file))?;
    let mut options = DiffOptions::new();
    options.context_lines(3);
    // A missing side (file added or deleted) diffs as empty.
    let at_bytes = at.as_ref().map_or(&[][..], Blob::content);
    let before = match against {
        Against::Previous => blob(&repo, commit.parents().next().and_then(|p| blob_id(&p, file)))?,
        Against::Working => None,
    };
    let patch = match against {
        Against::Previous => {
            Patch::from_buffers(before.as_ref().map_or(&[][..], Blob::content), Some(file), at_bytes, Some(file), Some(&mut options))?
        }
        Against::Working => Patch::from_buffers(at_bytes, Some(file), working.unwrap_or_default(), Some(file), Some(&mut options))?,
    };
    Ok(to_diff(&patch))
}

/// Files that were deleted and aren't in the vault now, most recent first.
/// `exists` says whether a path is present on disk.
pub fn deleted_files(root: &Path, limit: usize, exists: impl Fn(&str) -> bool) -> Result<Vec<DeletedFile>> {
    let repo = Repository::open(root)?;
    let Some(mut walk) = revwalk(&repo)? else { return Ok(Vec::new()) };
    walk.set_sorting(Sort::TIME)?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for oid in walk.take(DELETED_SCAN_COMMITS) {
        let commit = repo.find_commit(oid?)?;
        let Ok(parent) = commit.parent(0) else { continue };
        let diff = repo.diff_tree_to_tree(Some(&parent.tree()?), Some(&commit.tree()?), None)?;
        for delta in diff.deltas().filter(|d| d.status() == git2::Delta::Deleted) {
            let Some(path) = delta.old_file().path().map(|p| p.to_string_lossy().replace('\\', "/")) else { continue };
            // Only the latest deletion of a path matters, and only if it's still gone.
            if !seen.insert(path.clone()) || exists(&path) {
                continue;
            }
            out.push(DeletedFile {
                path,
                deleted_in: oid_string(commit.id()),
                restore_from: oid_string(parent.id()),
                time_ms: commit.time().seconds() * 1000,
                author: commit.author().name().unwrap_or("Unknown").to_string(),
            });
            if out.len() >= limit {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

/// A revwalk from HEAD, or None before the first commit.
fn revwalk(repo: &Repository) -> Result<Option<git2::Revwalk<'_>>> {
    if repo.head().is_err() {
        return Ok(None);
    }
    let mut walk = repo.revwalk()?;
    walk.push_head()?;
    Ok(Some(walk))
}

fn find_commit<'r>(repo: &'r Repository, spec: &str) -> Result<Commit<'r>> {
    Ok(repo.revparse_single(spec)?.peel_to_commit()?)
}

fn blob_id(commit: &Commit, path: &Path) -> Option<Oid> {
    commit.tree().ok()?.get_path(path).ok().filter(|e| e.kind() == Some(git2::ObjectType::Blob)).map(|e| e.id())
}

fn blob(repo: &Repository, id: Option<Oid>) -> Result<Option<Blob<'_>>> {
    Ok(id.map(|id| repo.find_blob(id)).transpose()?)
}

fn stats(repo: &Repository, before: Option<Oid>, after: Option<Oid>, path: &str) -> Result<(Option<usize>, Option<usize>)> {
    let (before, after) = (blob(repo, before)?, blob(repo, after)?);
    if before.as_ref().is_some_and(Blob::is_binary) || after.as_ref().is_some_and(Blob::is_binary) {
        return Ok((None, None));
    }
    let file = Path::new(path);
    let bytes = |b: &Option<Blob>| b.as_ref().map_or(Vec::new(), |b| b.content().to_vec());
    let (old, new) = (bytes(&before), bytes(&after));
    let patch = Patch::from_buffers(&old, Some(file), &new, Some(file), None)?;
    let (_, additions, deletions) = patch.line_stats()?;
    Ok((Some(additions), Some(deletions)))
}

fn to_diff(patch: &Patch) -> FileDiff {
    let mut diff = FileDiff::default();
    if patch.delta().flags().is_binary() {
        diff.binary = true;
        return diff;
    }
    if let Ok((_, additions, deletions)) = patch.line_stats() {
        diff.additions = additions;
        diff.deletions = deletions;
    }
    let mut shown = 0;
    'hunks: for h in 0..patch.num_hunks() {
        let Ok((hunk, count)) = patch.hunk(h) else { continue };
        let mut out = Hunk { old_start: hunk.old_start(), new_start: hunk.new_start(), lines: Vec::with_capacity(count) };
        for l in 0..count {
            let Ok(line) = patch.line_in_hunk(h, l) else { continue };
            let kind = match line.origin() {
                '+' | '-' | ' ' => line.origin(),
                _ => continue, // "\ No newline at end of file" and similar markers
            };
            if shown >= MAX_DIFF_LINES {
                diff.truncated = true;
                diff.hunks.push(out);
                break 'hunks;
            }
            shown += 1;
            out.lines.push(DiffLine {
                kind,
                text: String::from_utf8_lossy(line.content()).trim_end_matches(['\n', '\r']).to_string(),
                old_line: line.old_lineno(),
                new_line: line.new_lineno(),
            });
        }
        diff.hunks.push(out);
    }
    diff
}

fn oid_string(oid: Oid) -> String {
    oid.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::{GitTarget, sync};
    use crate::Author;
    use std::fs;

    fn commit(root: &Path) {
        let server = root.with_extension("server");
        if !server.exists() {
            Repository::init_bare(&server).unwrap();
        }
        let url = server.to_string_lossy().into_owned();
        let author = Author { name: "Tester".into(), email: "t@example.com".into() };
        sync(root, &GitTarget { url: &url, branch: "main", auth: None }, &author, "laptop").unwrap();
    }

    #[test]
    fn lists_versions_diffs_and_deleted_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("vault");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("Note.md"), "one\ntwo\n").unwrap();
        fs::write(root.join("Other.md"), "x").unwrap();
        commit(&root);
        fs::write(root.join("Note.md"), "one\nTWO\nthree\n").unwrap();
        fs::remove_file(root.join("Other.md")).unwrap();
        commit(&root);

        let versions = file_history(&root, "Note.md", 50).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].kind, ChangeKind::Modified);
        assert_eq!((versions[0].additions, versions[0].deletions), (Some(2), Some(1)));
        assert_eq!(versions[1].kind, ChangeKind::Added);
        assert_eq!(read_version(&root, &versions[1].commit, "Note.md").unwrap(), b"one\ntwo\n");

        let d = diff_version(&root, &versions[0].commit, "Note.md", Against::Previous, None).unwrap();
        let changed: Vec<String> = d.hunks.iter().flat_map(|h| &h.lines).filter(|l| l.kind != ' ').map(|l| format!("{}{}", l.kind, l.text)).collect();
        assert_eq!(changed, vec!["-two", "+TWO", "+three"]);

        fs::write(root.join("Note.md"), "one\nTWO\nthree\nfour\n").unwrap();
        let disk = fs::read(root.join("Note.md")).unwrap();
        let d = diff_version(&root, "HEAD", "Note.md", Against::Working, Some(&disk)).unwrap();
        assert_eq!((d.additions, d.deletions), (1, 0));

        let deleted = deleted_files(&root, 50, |p| root.join(p).exists()).unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].path, "Other.md");
        assert_eq!(read_version(&root, &deleted[0].restore_from, "Other.md").unwrap(), b"x");
    }

    #[test]
    fn empty_repository_has_no_history() {
        let dir = tempfile::tempdir().unwrap();
        Repository::init(dir.path()).unwrap();
        assert!(file_history(dir.path(), "Note.md", 10).unwrap().is_empty());
        assert!(deleted_files(dir.path(), 10, |_| false).unwrap().is_empty());
    }
}
