//! Git sync for one vault: commit everything, fetch, integrate, push.
//!
//! Git is used as sync with history rather than as a developer workflow:
//! there's no staging, every sync commits all changes, and overlapping edits
//! never leave conflict markers in notes. Like the Drive engine, both versions
//! are kept: ours at the original path, theirs as a "(conflict …)" copy.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use git2::build::CheckoutBuilder;
use git2::{
    Commit, Cred, CredentialType, Diff, ErrorCode, FetchOptions, IndexAddOption, IndexEntry, Oid, PushOptions,
    RemoteCallbacks, Repository, RepositoryInitOptions, Signature,
};
use serde::Serialize;

use crate::{Error, Result};

const REMOTE: &str = "origin";
/// Never committed: per-device sync state and the vault's local trash.
const EXCLUDES: &[&str] = &[".asteroid/", ".trash/", ".DS_Store"];
/// How often a push may be retried when another device pushed in between.
const PUSH_ATTEMPTS: usize = 3;
/// Unpushed changes larger than this are uploaded as a chain of smaller
/// commits. One huge push (say, a first upload of a PDF-heavy vault) keeps the
/// connection idle while libgit2 builds the pack, and GitHub drops it.
const PUSH_BATCH_BYTES: u64 = if cfg!(test) { 64 } else { 40 * 1024 * 1024 };
/// Local ref each batch is pushed from, so the branch itself never moves back.
const PUSH_REF: &str = "refs/asteroid/push";

/// GitHub's limits (docs.github.com, "About large files on GitHub"): files
/// over 50 MiB push with a warning, files over 100 MiB are rejected, and
/// repositories should ideally stay under 1 GB.
pub const WARN_FILE_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_FILE_BYTES: u64 = if cfg!(test) { 1000 } else { 100 * 1024 * 1024 };
pub const RECOMMENDED_REPO_BYTES: u64 = 1024 * 1024 * 1024;

/// HTTPS credentials. For GitHub, `username` is the account's login and
/// `token` an OAuth access token used as the password.
#[derive(Debug, Clone)]
pub struct Auth {
    pub username: String,
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct Author {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone)]
pub struct GitTarget<'a> {
    pub url: &'a str,
    pub branch: &'a str,
    pub auth: Option<&'a Auth>,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitReport {
    /// Paths included in this device's commit.
    pub committed: Vec<String>,
    /// Paths changed on disk by integrating remote commits.
    pub pulled: Vec<String>,
    /// Conflict copies created (theirs, next to our version).
    pub conflicts: Vec<String>,
    /// Files left out of commits because GitHub would reject them, with sizes.
    pub too_large: Vec<SizedFile>,
    pub pushed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SizedFile {
    pub path: String,
    pub bytes: u64,
}

/// How a vault measures up against GitHub's size limits.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeReport {
    /// Files git would sync (visible and not ignored) and their total size.
    pub file_count: usize,
    pub total_bytes: u64,
    /// Size of `.git` (all history), 0 before the first sync.
    pub history_bytes: u64,
    /// Files at or over `warn_file_bytes`, largest first.
    pub large_files: Vec<SizedFile>,
    pub warn_file_bytes: u64,
    pub max_file_bytes: u64,
    pub recommended_repo_bytes: u64,
    /// Files left out by `.gitignore` (synced or not, they stay on disk).
    pub ignored_count: usize,
    pub ignored_bytes: u64,
    /// Every visible file grouped by extension, ignored or not, largest first.
    pub by_extension: Vec<ExtensionStat>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionStat {
    /// Lower-case, without the dot; empty for files without one.
    pub ext: String,
    pub count: usize,
    pub bytes: u64,
}

/// Measures the vault against GitHub's limits without changing anything.
pub fn size_report(root: &Path) -> SizeReport {
    let repo = Repository::open(root).ok();
    let mut report = SizeReport {
        file_count: 0,
        total_bytes: 0,
        history_bytes: 0,
        large_files: Vec::new(),
        warn_file_bytes: WARN_FILE_BYTES,
        max_file_bytes: MAX_FILE_BYTES,
        recommended_repo_bytes: RECOMMENDED_REPO_BYTES,
        ignored_count: 0,
        ignored_bytes: 0,
        by_extension: Vec::new(),
    };
    // Before the first sync there's no repository to ask, so apply Asteroid's
    // own rules directly; that's a close estimate of what would upload.
    let rules = repo.is_none().then(|| crate::ignore::read_rules(root));
    let mut by_ext = std::collections::HashMap::<String, (usize, u64)>::new();
    let files = walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file());
    for entry in files {
        let Ok(rel) = entry.path().strip_prefix(root) else { continue };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let ext = rel_str.rsplit_once('.').filter(|(stem, _)| !stem.ends_with('/') && !stem.is_empty()).map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        let stat = by_ext.entry(ext).or_default();
        stat.0 += 1;
        stat.1 += bytes;
        let ignored = match (&repo, &rules) {
            (Some(r), _) => r.is_path_ignored(rel).unwrap_or(false),
            (None, Some(rules)) => rules.matches(&rel_str),
            (None, None) => false,
        };
        if ignored {
            report.ignored_count += 1;
            report.ignored_bytes += bytes;
            continue;
        }
        report.file_count += 1;
        report.total_bytes += bytes;
        if bytes >= WARN_FILE_BYTES.min(MAX_FILE_BYTES) {
            report.large_files.push(SizedFile { path: rel.to_string_lossy().replace('\\', "/"), bytes });
        }
    }
    report.large_files.sort_by_key(|f| std::cmp::Reverse(f.bytes));
    report.by_extension = by_ext.into_iter().map(|(ext, (count, bytes))| ExtensionStat { ext, count, bytes }).collect();
    report.by_extension.sort_by_key(|e| std::cmp::Reverse(e.bytes));
    report.history_bytes = walkdir::WalkDir::new(root.join(".git"))
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum();
    report
}

/// Opens the vault's repository, creating it on `branch` if there is none,
/// and points `origin` at `url`. Refuses to repoint an `origin` that already
/// names a different repository, or to work on a detached HEAD.
pub fn prepare(root: &Path, url: &str, branch: &str) -> Result<Repository> {
    let repo = match Repository::open(root) {
        Ok(repo) => repo,
        Err(e) if e.code() == ErrorCode::NotFound => {
            Repository::init_opts(root, RepositoryInitOptions::new().initial_head(branch).mkdir(false))?
        }
        Err(e) => return Err(e.into()),
    };
    if repo.workdir().map(|w| w.canonicalize().ok() != root.canonicalize().ok()).unwrap_or(true) {
        return Err(Error::Setup("The vault is inside another git repository; open the repository's top folder instead.".into()));
    }
    if repo.head_detached().unwrap_or(false) {
        return Err(Error::Setup("The vault's repository has no branch checked out.".into()));
    }
    match repo.find_remote(REMOTE) {
        Ok(remote) if remote.url().ok() == Some(url) => {}
        Ok(remote) => {
            return Err(Error::Setup(format!(
                "This folder is already a git repository whose remote is {}.",
                remote.url().unwrap_or("something else")
            )));
        }
        Err(_) => {
            repo.remote(REMOTE, url)?;
        }
    }
    exclude_meta(&repo)?;
    Ok(repo)
}

/// The branch HEAD points at, even before the first commit.
pub fn current_branch(root: &Path) -> Option<String> {
    let repo = Repository::open(root).ok()?;
    let head = repo.find_reference("HEAD").ok()?;
    head.symbolic_target().ok()??.strip_prefix("refs/heads/").map(String::from)
}

/// `user.name` / `user.email` from the repository's or the user's git config.
pub fn configured_author(root: &Path) -> Option<Author> {
    let config = Repository::open(root).and_then(|r| r.config()).or_else(|_| git2::Config::open_default()).ok()?;
    Some(Author { name: config.get_string("user.name").ok()?, email: config.get_string("user.email").ok()? })
}

/// Clones `target` into `dest`, which must be empty or missing.
pub fn clone(target: &GitTarget, dest: &Path) -> Result<()> {
    let attempts = AuthAttempts::default();
    let mut fetch = FetchOptions::new();
    fetch.remote_callbacks(callbacks(target.auth, &attempts, None));
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch);
    let repo = match builder.clone(target.url, dest) {
        Ok(repo) => repo,
        Err(e) => return Err(network_error("download the repository", e, &attempts)),
    };
    // Make sure the requested branch is the one checked out: the remote's
    // HEAD may name another branch, or nothing at all for an empty repository.
    let local = format!("refs/heads/{}", target.branch);
    if repo.head().ok().and_then(|h| h.name().ok().map(String::from)) != Some(local.clone()) {
        match repo.refname_to_id(&format!("refs/remotes/{REMOTE}/{}", target.branch)) {
            Ok(oid) => {
                repo.reference(&local, oid, true, "asteroid: clone")?;
                repo.set_head(&local)?;
                // A fresh clone has no local edits to lose.
                repo.checkout_head(Some(CheckoutBuilder::new().force()))?;
            }
            Err(_) => repo.set_head(&local)?,
        }
    }
    exclude_meta(&repo)?;
    Ok(())
}

/// One full sync. Safe to call repeatedly; a no-op when nothing changed.
pub fn sync(root: &Path, target: &GitTarget, author: &Author, device: &str) -> Result<GitReport> {
    let repo = prepare(root, target.url, target.branch)?;
    let sig = Signature::now(&author.name, &author.email)?;
    let mut report = GitReport::default();
    let local_ref = format!("refs/heads/{}", target.branch);
    let tracking_ref = format!("refs/remotes/{REMOTE}/{}", target.branch);

    (report.committed, report.too_large) = commit_all(&repo, &sig, device)?;

    for attempt in 1..=PUSH_ATTEMPTS {
        fetch(&repo, target)?;
        let theirs = repo.refname_to_id(&tracking_ref).ok();
        let ours = repo.refname_to_id(&local_ref).ok();
        integrate(&repo, &sig, &local_ref, ours, theirs, &mut report)?;

        let ours = repo.refname_to_id(&local_ref).ok();
        if ours.is_none() || ours == theirs {
            return Ok(report);
        }
        let steps = batches(&repo, &sig, theirs, ours.unwrap())?;
        // The branch takes the final commit (same tree as before) up front, so
        // an interrupted upload leaves nothing behind locally; the next sync
        // pushes whatever the remote is still missing.
        repo.reference(&local_ref, *steps.last().unwrap(), true, "asteroid: split push")?;
        let mut result = Ok(());
        for step in steps {
            repo.reference(PUSH_REF, step, true, "asteroid: push")?;
            result = push(&repo, target, PUSH_REF);
            if result.is_err() {
                break;
            }
            // Record what the remote now has so the next sync starts from it.
            repo.reference(&tracking_ref, step, true, "asteroid: push")?;
            report.pushed = true;
        }
        if let Ok(mut r) = repo.find_reference(PUSH_REF) {
            let _ = r.delete();
        }
        match result {
            Ok(()) => return Ok(report),
            // Another device pushed in between: fetch, merge and try again.
            Err(Error::Rejected(_)) if attempt < PUSH_ATTEMPTS => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!("the last attempt returns")
}

/// Whether the unpushed commits contain file versions that neither `tip` nor
/// the remote has, e.g. files excluded or deleted before they were ever
/// pushed. Pushing those commits as they are would upload them anyway, so
/// the history is rebuilt from `tip`'s content instead.
fn carries_dropped_content(
    repo: &Repository,
    base: Option<Oid>,
    base_tree: Option<&git2::Tree>,
    tip: Oid,
    changes: &[Change],
) -> Result<bool> {
    let mut known: HashSet<Oid> = changes.iter().filter_map(|c| c.blob.map(|(id, _, _)| id)).collect();
    if let Some(tree) = base_tree {
        tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
            if entry.kind() == Some(git2::ObjectType::Blob) {
                known.insert(entry.id());
            }
            git2::TreeWalkResult::Ok
        })?;
    }
    let mut walk = repo.revwalk()?;
    walk.push(tip)?;
    if let Some(base) = base {
        walk.hide(base)?;
    }
    for oid in walk {
        let commit = repo.find_commit(oid?)?;
        let parent_tree = commit.parent(0).ok().map(|p| p.tree()).transpose()?;
        let diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&commit.tree()?), None)?;
        for delta in diff.deltas().filter(|d| d.status() != git2::Delta::Deleted) {
            if delta.new_file().mode() != git2::FileMode::Commit && !known.contains(&delta.new_file().id()) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// One path's change in a push: its new blob, or `None` for a deletion.
struct Change {
    path: String,
    blob: Option<(Oid, u32, u64)>,
}

impl Change {
    fn bytes(&self) -> u64 {
        self.blob.map_or(0, |(_, _, size)| size)
    }
}

/// The commits to push to get the remote from `base` to `tip`. Usually just
/// `tip`; when the new content is large, the unpushed history is replaced by
/// a chain of commits from `base` that each add a slice of it and end in
/// exactly `tip`'s tree. Unpushed commits exist only here, so rewriting them
/// is safe.
fn batches(repo: &Repository, sig: &Signature, base: Option<Oid>, tip: Oid) -> Result<Vec<Oid>> {
    let base_commit = base.map(|b| repo.find_commit(b)).transpose()?;
    let base_tree = base_commit.as_ref().map(|c| c.tree()).transpose()?;
    let tip_commit = repo.find_commit(tip)?;
    let tip_tree = tip_commit.tree()?;
    let diff = repo.diff_tree_to_tree(base_tree.as_ref(), Some(&tip_tree), None)?;

    let mut changes = Vec::new();
    for delta in diff.deltas() {
        let file = if delta.status() == git2::Delta::Deleted { delta.old_file() } else { delta.new_file() };
        let Some(path) = file.path().map(|p| p.to_string_lossy().replace('\\', "/")) else { continue };
        let blob = (delta.status() != git2::Delta::Deleted).then(|| {
            let size = repo.find_blob(file.id()).map(|b| b.size() as u64).unwrap_or(0);
            (file.id(), u32::from(file.mode()), size)
        });
        changes.push(Change { path, blob });
    }
    let small = changes.iter().map(Change::bytes).sum::<u64>() <= PUSH_BATCH_BYTES;
    if small && !carries_dropped_content(repo, base, base_tree.as_ref(), tip, &changes)? {
        return Ok(vec![tip]);
    }
    // Deletions first, then by path, so parts are deterministic.
    changes.sort_by(|a, b| (a.blob.is_some(), &a.path).cmp(&(b.blob.is_some(), &b.path)));

    let mut groups: Vec<Vec<&Change>> = vec![Vec::new()];
    let mut size = 0;
    for change in &changes {
        let bytes = change.bytes();
        if size > 0 && size + bytes > PUSH_BATCH_BYTES {
            groups.push(Vec::new());
            size = 0;
        }
        size += bytes;
        groups.last_mut().unwrap().push(change);
    }

    let mut index = git2::Index::new()?;
    if let Some(tree) = &base_tree {
        index.read_tree(tree)?;
    }
    let summary = tip_commit.summary().ok().flatten().unwrap_or("Asteroid: sync").to_string();
    let mut parent = base_commit;
    let mut steps = Vec::with_capacity(groups.len());
    let count = groups.len();
    for (i, group) in groups.into_iter().enumerate() {
        for Change { path, blob } in group {
            match blob {
                None => index.remove_path(Path::new(path))?,
                Some((id, mode, size)) => index.add(&IndexEntry {
                    ctime: git2::IndexTime::new(0, 0),
                    mtime: git2::IndexTime::new(0, 0),
                    dev: 0,
                    ino: 0,
                    mode: *mode,
                    uid: 0,
                    gid: 0,
                    file_size: *size as u32,
                    id: *id,
                    flags: path.len().min(0xFFF) as u16,
                    flags_extended: 0,
                    path: path.as_bytes().to_vec(),
                })?,
            }
        }
        // The last step reuses the tip's tree so the result is exactly the same.
        let tree = if i + 1 == count { tip_tree.clone() } else { repo.find_tree(index.write_tree_to(repo)?)? };
        let message = format!("{summary} (part {}/{count})", i + 1);
        let parents: Vec<&Commit> = parent.iter().collect();
        let id = repo.commit(None, sig, sig, &message, &tree, &parents)?;
        parent = Some(repo.find_commit(id)?);
        steps.push(id);
    }
    Ok(steps)
}

/// Commits every change in the working tree. Returns the changed paths.
/// Commits every change in the working tree, except files too large for
/// GitHub: once in history, one of those would make every push fail.
/// Returns the changed paths and the files left out.
fn commit_all(repo: &Repository, sig: &Signature, device: &str) -> Result<(Vec<String>, Vec<SizedFile>)> {
    let workdir = repo.workdir().ok_or_else(|| Error::Setup("The vault's repository has no working folder.".into()))?.to_path_buf();
    let mut too_large = Vec::new();
    let mut skip_large = |path: &Path, _spec: &[u8]| -> i32 {
        let bytes = fs::metadata(workdir.join(path)).map(|m| m.len()).unwrap_or(0);
        if bytes <= MAX_FILE_BYTES {
            return 0;
        }
        let path = path.to_string_lossy().replace('\\', "/");
        if !too_large.iter().any(|f: &SizedFile| f.path == path) {
            too_large.push(SizedFile { path, bytes });
        }
        1
    };
    let mut index = repo.index()?;
    index.add_all(["*"], IndexAddOption::DEFAULT, Some(&mut skip_large))?;
    index.update_all(["*"], Some(&mut skip_large))?;
    // Tracked files that .gitignore now covers stop being tracked (like
    // `git rm --cached`): they stay on disk but leave the repository. Other
    // devices keep their copies too; see `keep_ignored`.
    let ignored: Vec<String> = index
        .iter()
        .map(|e| String::from_utf8_lossy(&e.path).into_owned())
        .filter(|p| repo.is_path_ignored(p).unwrap_or(false))
        .collect();
    for path in &ignored {
        index.remove_path(Path::new(path))?;
    }
    index.write()?;
    let tree = repo.find_tree(index.write_tree()?)?;
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parent_tree = parent.as_ref().map(|p| p.tree()).transpose()?;
    let diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)?;
    let changed = paths(&diff);
    if changed.is_empty() {
        return Ok((changed, too_large));
    }
    let noun = if changed.len() == 1 { "file" } else { "files" };
    let message = format!("Asteroid: {} {noun} changed on {device}", changed.len());
    let parents: Vec<&Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), sig, sig, &message, &tree, &parents)?;
    Ok((changed, too_large))
}

fn fetch(repo: &Repository, target: &GitTarget) -> Result<()> {
    let attempts = AuthAttempts::default();
    let mut options = FetchOptions::new();
    options.remote_callbacks(callbacks(target.auth, &attempts, None));
    let refspec = format!("+refs/heads/{0}:refs/remotes/{REMOTE}/{0}", target.branch);
    let mut remote = repo.find_remote(REMOTE)?;
    match remote.fetch(&[refspec], Some(&mut options), None) {
        Ok(()) => Ok(()),
        // The remote has no such branch yet (e.g. a brand-new empty repository).
        Err(e) if e.code() == ErrorCode::NotFound => Ok(()),
        Err(e) => Err(network_error("fetch from GitHub", e, &attempts)),
    }
}

/// Brings remote commits into the local branch and the working tree.
fn integrate(
    repo: &Repository,
    sig: &Signature,
    local_ref: &str,
    ours: Option<Oid>,
    theirs: Option<Oid>,
    report: &mut GitReport,
) -> Result<()> {
    let Some(theirs) = theirs else { return Ok(()) };
    let target = match ours {
        // Nothing local yet: adopt the remote branch as is.
        None => theirs,
        Some(ours) if ours == theirs || repo.graph_descendant_of(ours, theirs)? => return Ok(()),
        Some(ours) if repo.graph_descendant_of(theirs, ours)? => theirs,
        Some(ours) => merge(repo, sig, ours, theirs, report)?,
    };
    let before = ours.map(|o| repo.find_commit(o)?.tree()).transpose()?;
    let after = repo.find_commit(target)?.tree()?;
    let diff = repo.diff_tree_to_tree(before.as_ref(), Some(&after), None)?;
    let kept = keep_ignored(repo, &diff, || {
        // Safe checkout: refuses to overwrite a file edited since commit_all,
        // in which case this sync fails and the next one picks the edit up.
        repo.checkout_tree(after.as_object(), Some(CheckoutBuilder::new().safe()))?;
        Ok(())
    })?;
    repo.reference(local_ref, target, true, "asteroid: sync")?;
    report.pulled.extend(paths(&diff).into_iter().filter(|p| !kept.contains(p)));
    Ok(())
}

/// Runs `checkout`, but keeps files the incoming change deletes only because
/// another device stopped syncing them (they're ignored once the new
/// `.gitignore` is in place). Such a file is moved aside first and put back
/// afterwards; a real deletion is carried out. Files edited since the last
/// commit aren't touched, so the safe checkout still protects them.
fn keep_ignored(repo: &Repository, diff: &Diff, checkout: impl FnOnce() -> Result<()>) -> Result<Vec<String>> {
    let workdir = repo.workdir().ok_or_else(|| Error::Setup("The vault's repository has no working folder.".into()))?.to_path_buf();
    let aside = repo.path().join("asteroid-keep");
    let mut moved = Vec::new();
    for delta in diff.deltas().filter(|d| d.status() == git2::Delta::Deleted) {
        let Some(path) = delta.old_file().path() else { continue };
        let file = workdir.join(path);
        let unchanged = Oid::hash_file(git2::ObjectType::Blob, &file).is_ok_and(|id| id == delta.old_file().id());
        if !unchanged {
            continue;
        }
        let dest = aside.join(path);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::rename(&file, &dest)?;
        moved.push(path.to_path_buf());
    }
    let result = checkout();
    let mut kept = Vec::new();
    for path in moved {
        let (from, to) = (aside.join(&path), workdir.join(&path));
        // Put files back if the checkout failed, or if they're now ignored.
        if result.is_err() || repo.is_path_ignored(&path).unwrap_or(false) {
            fs::create_dir_all(to.parent().unwrap())?;
            fs::rename(&from, &to)?;
            kept.push(path.to_string_lossy().replace('\\', "/"));
        } else {
            fs::remove_file(&from)?;
        }
    }
    let _ = fs::remove_dir_all(&aside);
    result.map(|()| kept)
}

/// Merges two diverged commits, keeping both versions of overlapping edits.
fn merge(repo: &Repository, sig: &Signature, ours: Oid, theirs: Oid, report: &mut GitReport) -> Result<Oid> {
    let ours = repo.find_commit(ours)?;
    let theirs = repo.find_commit(theirs)?;
    let mut index = repo.merge_commits(&ours, &theirs, None)?;
    if index.has_conflicts() {
        let conflicts: Vec<_> = index.conflicts()?.collect::<std::result::Result<_, _>>()?;
        for conflict in conflicts {
            let path = conflict
                .our
                .as_ref()
                .or(conflict.their.as_ref())
                .or(conflict.ancestor.as_ref())
                .map(|e| String::from_utf8_lossy(&e.path).into_owned())
                .ok_or_else(|| Error::Setup("Empty merge conflict".into()))?;
            index.conflict_remove(Path::new(&path))?;
            match (conflict.our, conflict.their) {
                // Edited on both sides: ours stays, theirs becomes a copy.
                (Some(our), Some(their)) => {
                    index.add(&resolved(our, None))?;
                    let copy = conflict_path(&index, &path);
                    index.add(&resolved(their, Some(&copy)))?;
                    report.conflicts.push(copy);
                }
                // Edited on one side, deleted on the other: the edit wins.
                (Some(kept), None) | (None, Some(kept)) => index.add(&resolved(kept, None))?,
                (None, None) => {}
            }
        }
    }
    let tree = repo.find_tree(index.write_tree_to(repo)?)?;
    let message = format!("Asteroid: merge changes from {}", theirs.author().name().unwrap_or("another device"));
    Ok(repo.commit(None, sig, sig, &message, &tree, &[&ours, &theirs])?)
}

/// A conflict entry moved to stage 0, optionally under a new path.
fn resolved(mut entry: IndexEntry, path: Option<&str>) -> IndexEntry {
    if let Some(path) = path {
        entry.path = path.as_bytes().to_vec();
    }
    // Low 12 bits: path length; bits 12-13: merge stage (cleared).
    entry.flags = (entry.path.len().min(0xFFF)) as u16;
    entry
}

/// "dir/Note.md" → "dir/Note (conflict 2026-10-30 1215).md", unused in `index`.
fn conflict_path(index: &git2::Index, path: &str) -> String {
    let (dir, name) = path.rsplit_once('/').map_or(("", path), |(d, n)| (d, n));
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (name, String::new()),
    };
    let stamp = chrono::Local::now().format("%Y-%m-%d %H%M");
    let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
    (1..)
        .map(|n| match n {
            1 => format!("{prefix}{stem} (conflict {stamp}){ext}"),
            n => format!("{prefix}{stem} (conflict {stamp} {n}){ext}"),
        })
        .find(|candidate| index.get_path(Path::new(candidate), 0).is_none())
        .unwrap()
}

fn push(repo: &Repository, target: &GitTarget, local_ref: &str) -> Result<()> {
    let attempts = AuthAttempts::default();
    let rejected = RefCell::new(None);
    let mut options = PushOptions::new();
    options.remote_callbacks(callbacks(target.auth, &attempts, Some(&rejected)));
    let mut remote = repo.find_remote(REMOTE)?;
    let refspec = format!("{local_ref}:refs/heads/{}", target.branch);
    match remote.push(&[refspec], Some(&mut options)) {
        Err(e) if e.code() == ErrorCode::NotFastForward => Err(Error::Rejected(e.message().into())),
        Err(e) => Err(network_error("push to GitHub", e, &attempts)),
        Ok(()) => match rejected.borrow_mut().take() {
            Some(reason) => Err(Error::Rejected(reason)),
            None => Ok(()),
        },
    }
}

/// What happened with credentials during one network operation, so errors
/// can say whether the sign-in was refused or something else went wrong.
#[derive(Default)]
struct AuthAttempts {
    count: Cell<usize>,
    /// Every username was tried and the server still asked again.
    refused: Cell<bool>,
    /// The server asked for credentials but there were none to give.
    missing: Cell<bool>,
}

/// Supplies credentials and records server-side push rejections. libgit2
/// asks again after a refusal; each username is offered once, then the
/// operation fails instead of looping. The login comes first (what git
/// credential managers pair with OAuth tokens), then `x-access-token`.
fn callbacks<'a>(
    auth: Option<&'a Auth>,
    attempts: &'a AuthAttempts,
    rejected: Option<&'a RefCell<Option<String>>>,
) -> RemoteCallbacks<'a> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |_url, _username, allowed| {
        let Some(auth) = auth.filter(|_| allowed.contains(CredentialType::USER_PASS_PLAINTEXT)) else {
            attempts.missing.set(true);
            return Err(git2::Error::from_str("authentication required"));
        };
        let mut names = vec![auth.username.as_str(), "x-access-token"];
        names.dedup();
        let n = attempts.count.get();
        attempts.count.set(n + 1);
        match names.get(n) {
            Some(name) => Cred::userpass_plaintext(name, &auth.token),
            None => {
                attempts.refused.set(true);
                Err(git2::Error::from_str("authentication failed"))
            }
        }
    });
    if let Some(rejected) = rejected {
        callbacks.push_update_reference(move |_refname, status| {
            if let Some(reason) = status {
                *rejected.borrow_mut() = Some(reason.to_string());
            }
            Ok(())
        });
    }
    callbacks
}

fn network_error(op: &str, e: git2::Error, attempts: &AuthAttempts) -> Error {
    if attempts.refused.get() {
        Error::Auth(format!("GitHub refused the sign-in while trying to {op}. Reconnect GitHub in Settings › Sync."))
    } else if attempts.missing.get() {
        Error::Auth("This repository needs a GitHub sign-in. Connect GitHub in Settings › Sync.".into())
    } else {
        Error::Network(format!("Couldn't {op}: {}", e.message()))
    }
}

fn paths(diff: &Diff) -> Vec<String> {
    diff.deltas()
        .filter_map(|d| d.new_file().path().or_else(|| d.old_file().path()))
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect()
}

/// Keeps device-local folders out of commits via `.git/info/exclude`, which,
/// unlike `.gitignore`, isn't itself part of the vault's history.
fn exclude_meta(repo: &Repository) -> Result<()> {
    let path = repo.path().join("info").join("exclude");
    let current = fs::read_to_string(&path).unwrap_or_default();
    let missing: Vec<&str> = EXCLUDES.iter().copied().filter(|e| !current.lines().any(|l| l.trim() == *e)).collect();
    if missing.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(path.parent().unwrap())?;
    let mut text = current;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str("# Added by Asteroid\n");
    for e in missing {
        text.push_str(e);
        text.push('\n');
    }
    fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn author(name: &str) -> Author {
        Author { name: name.into(), email: format!("{name}@example.com") }
    }

    struct Setup {
        _dirs: Vec<TempDir>,
        url: String,
        a: std::path::PathBuf,
        b: std::path::PathBuf,
    }

    /// A bare "server" repository and two empty device folders.
    fn setup() -> Setup {
        let server = tempfile::tempdir().unwrap();
        Repository::init_bare(server.path()).unwrap();
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        Setup {
            url: server.path().to_string_lossy().into_owned(),
            a: a.path().to_path_buf(),
            b: b.path().to_path_buf(),
            _dirs: vec![server, a, b],
        }
    }

    fn sync_dev(root: &Path, url: &str, who: &str) -> GitReport {
        let target = GitTarget { url, branch: "main", auth: None };
        sync(root, &target, &author(who), who).unwrap()
    }

    fn read(root: &Path, path: &str) -> String {
        fs::read_to_string(root.join(path)).unwrap()
    }

    #[test]
    fn two_devices_converge() {
        let s = setup();
        fs::write(s.a.join("Note.md"), "hello\n").unwrap();
        fs::create_dir_all(s.a.join(".asteroid")).unwrap();
        fs::write(s.a.join(".asteroid/sync.json"), "{}").unwrap();
        let r = sync_dev(&s.a, &s.url, "laptop");
        assert_eq!(r.committed, vec!["Note.md"]);
        assert!(r.pushed);

        let r = sync_dev(&s.b, &s.url, "phone");
        assert_eq!(r.pulled, vec!["Note.md"]);
        assert_eq!(read(&s.b, "Note.md"), "hello\n");
        assert!(!s.b.join(".asteroid").exists(), "device metadata must not be committed");

        fs::write(s.b.join("Note.md"), "hello\nfrom phone\n").unwrap();
        fs::write(s.b.join("New.md"), "new").unwrap();
        sync_dev(&s.b, &s.url, "phone");
        let r = sync_dev(&s.a, &s.url, "laptop");
        assert_eq!(r.pulled.len(), 2);
        assert_eq!(read(&s.a, "Note.md"), "hello\nfrom phone\n");

        // Nothing changed anywhere: a no-op.
        let r = sync_dev(&s.a, &s.url, "laptop");
        assert!(r.committed.is_empty() && r.pulled.is_empty() && !r.pushed);
    }

    #[test]
    fn edits_to_different_lines_merge() {
        let s = setup();
        fs::write(s.a.join("Note.md"), "one\ntwo\nthree\nfour\nfive\n").unwrap();
        sync_dev(&s.a, &s.url, "laptop");
        sync_dev(&s.b, &s.url, "phone");

        fs::write(s.a.join("Note.md"), "ONE\ntwo\nthree\nfour\nfive\n").unwrap();
        fs::write(s.b.join("Note.md"), "one\ntwo\nthree\nfour\nFIVE\n").unwrap();
        sync_dev(&s.a, &s.url, "laptop");
        let r = sync_dev(&s.b, &s.url, "phone");
        assert!(r.conflicts.is_empty());
        assert!(r.pushed);
        assert_eq!(read(&s.b, "Note.md"), "ONE\ntwo\nthree\nfour\nFIVE\n");
        sync_dev(&s.a, &s.url, "laptop");
        assert_eq!(read(&s.a, "Note.md"), "ONE\ntwo\nthree\nfour\nFIVE\n");
    }

    #[test]
    fn overlapping_edits_keep_both_versions() {
        let s = setup();
        fs::write(s.a.join("Note.md"), "draft\n").unwrap();
        sync_dev(&s.a, &s.url, "laptop");
        sync_dev(&s.b, &s.url, "phone");

        fs::write(s.a.join("Note.md"), "laptop version\n").unwrap();
        fs::write(s.b.join("Note.md"), "phone version\n").unwrap();
        sync_dev(&s.a, &s.url, "laptop");
        let r = sync_dev(&s.b, &s.url, "phone");
        assert_eq!(r.conflicts.len(), 1);
        assert!(r.conflicts[0].starts_with("Note (conflict "), "{:?}", r.conflicts);
        assert_eq!(read(&s.b, "Note.md"), "phone version\n");
        assert_eq!(read(&s.b, &r.conflicts[0]), "laptop version\n");
        assert!(!read(&s.b, "Note.md").contains("<<<<"));

        // The laptop ends up with both too.
        sync_dev(&s.a, &s.url, "laptop");
        assert_eq!(read(&s.a, &r.conflicts[0]), "laptop version\n");
        assert_eq!(read(&s.a, "Note.md"), "phone version\n");
    }

    #[test]
    fn deletions_propagate_and_edits_beat_deletes() {
        let s = setup();
        fs::write(s.a.join("Gone.md"), "x").unwrap();
        fs::write(s.a.join("Kept.md"), "y").unwrap();
        sync_dev(&s.a, &s.url, "laptop");
        sync_dev(&s.b, &s.url, "phone");

        fs::remove_file(s.a.join("Gone.md")).unwrap();
        fs::remove_file(s.a.join("Kept.md")).unwrap();
        fs::write(s.b.join("Kept.md"), "edited").unwrap();
        sync_dev(&s.a, &s.url, "laptop");
        sync_dev(&s.b, &s.url, "phone");
        assert!(!s.b.join("Gone.md").exists());
        assert_eq!(read(&s.b, "Kept.md"), "edited");
    }

    #[test]
    fn clone_then_sync() {
        let s = setup();
        fs::write(s.a.join("Note.md"), "hi").unwrap();
        sync_dev(&s.a, &s.url, "laptop");
        let dest = s.b.join("Vault");
        clone(&GitTarget { url: &s.url, branch: "main", auth: None }, &dest).unwrap();
        assert_eq!(read(&dest, "Note.md"), "hi");
        fs::write(dest.join("Note.md"), "hi again").unwrap();
        assert!(sync_dev(&dest, &s.url, "phone").pushed);
    }

    #[test]
    fn links_a_non_empty_vault_to_a_non_empty_repository() {
        let s = setup();
        fs::write(s.a.join("Remote.md"), "from the repo").unwrap();
        fs::write(s.a.join("Same.md"), "repo copy").unwrap();
        sync_dev(&s.a, &s.url, "laptop");

        // Device B already has its own notes and no shared history.
        fs::write(s.b.join("Local.md"), "from this device").unwrap();
        fs::write(s.b.join("Same.md"), "device copy").unwrap();
        let r = sync_dev(&s.b, &s.url, "phone");
        assert!(r.pushed);
        assert_eq!(read(&s.b, "Remote.md"), "from the repo");
        assert_eq!(read(&s.b, "Local.md"), "from this device");
        assert_eq!(read(&s.b, "Same.md"), "device copy");
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(read(&s.b, &r.conflicts[0]), "repo copy");
    }

    #[test]
    fn large_uploads_are_split_into_parts() {
        let s = setup();
        for i in 0..5 {
            fs::write(s.a.join(format!("Paper {i}.md")), "x".repeat(50)).unwrap();
        }
        let r = sync_dev(&s.a, &s.url, "laptop");
        assert!(r.pushed);
        let server = Repository::open_bare(&s.url).unwrap();
        let tip = server.find_reference("refs/heads/main").unwrap().peel_to_commit().unwrap();
        // 250 bytes in 64-byte batches: one file per part.
        assert_eq!(tip.summary().unwrap().unwrap(), "Asteroid: 5 files changed on laptop (part 5/5)");
        assert_eq!(tip.parent_count(), 1);
        let local = Repository::open(&s.a).unwrap();
        assert_eq!(local.head().unwrap().peel_to_commit().unwrap().id(), tip.id());
        assert!(local.find_reference(PUSH_REF).is_err());

        // The other device gets everything, and later small syncs push normally.
        sync_dev(&s.b, &s.url, "phone");
        assert_eq!(read(&s.b, "Paper 4.md"), "x".repeat(50));
        fs::write(s.b.join("Paper 0.md"), "edited").unwrap();
        assert!(sync_dev(&s.b, &s.url, "phone").pushed);
        sync_dev(&s.a, &s.url, "laptop");
        assert_eq!(read(&s.a, "Paper 0.md"), "edited");
    }

    #[test]
    fn files_over_the_github_limit_are_left_out() {
        let s = setup();
        fs::write(s.a.join("Small.md"), "ok").unwrap();
        fs::write(s.a.join("Huge.pdf"), vec![0u8; MAX_FILE_BYTES as usize + 1]).unwrap();
        let r = sync_dev(&s.a, &s.url, "laptop");
        assert_eq!(r.committed, vec!["Small.md"]);
        assert_eq!(r.too_large.len(), 1);
        assert_eq!(r.too_large[0].path, "Huge.pdf");
        sync_dev(&s.b, &s.url, "phone");
        assert!(s.b.join("Small.md").exists());
        assert!(!s.b.join("Huge.pdf").exists());

        let report = size_report(&s.a);
        assert_eq!(report.file_count, 2);
        assert_eq!(report.large_files[0].path, "Huge.pdf");
        assert!(report.history_bytes > 0);
    }

    #[test]
    fn excluding_a_file_type_keeps_files_on_every_device() {
        let s = setup();
        fs::write(s.a.join("Note.md"), "n").unwrap();
        fs::write(s.a.join("Paper.pdf"), "pdf").unwrap();
        sync_dev(&s.a, &s.url, "laptop");
        sync_dev(&s.b, &s.url, "phone");
        assert!(s.b.join("Paper.pdf").exists());

        // The laptop stops syncing PDFs.
        let rules = crate::ignore::IgnoreRules { extensions: vec!["pdf".into()], paths: vec![] };
        crate::ignore::write_rules(&s.a, &rules).unwrap();
        let r = sync_dev(&s.a, &s.url, "laptop");
        assert!(r.committed.contains(&"Paper.pdf".to_string()), "untracked in the repository: {:?}", r.committed);
        assert!(s.a.join("Paper.pdf").exists(), "kept on the excluding device");

        // The phone gets the new rules and keeps its own copy.
        let r = sync_dev(&s.b, &s.url, "phone");
        assert!(s.b.join("Paper.pdf").exists(), "kept on the receiving device");
        assert!(!r.pulled.contains(&"Paper.pdf".to_string()));
        assert!(s.b.join(".gitignore").exists());

        // New PDFs no longer upload; real deletions still propagate.
        fs::write(s.b.join("New.pdf"), "x").unwrap();
        fs::remove_file(s.b.join("Note.md")).unwrap();
        sync_dev(&s.b, &s.url, "phone");
        sync_dev(&s.a, &s.url, "laptop");
        assert!(!s.a.join("New.pdf").exists());
        assert!(!s.a.join("Note.md").exists());

        let report = size_report(&s.a);
        assert_eq!(report.ignored_count, 1);
        assert!(report.by_extension.iter().any(|e| e.ext == "pdf" && e.count == 1));
    }

    #[test]
    fn content_excluded_before_the_first_push_is_never_uploaded() {
        let s = setup();
        fs::write(s.a.join("Note.md"), "n").unwrap();
        fs::write(s.a.join("Paper.pdf"), "secret pdf").unwrap();
        // A commit that never reached the server (say, the upload failed).
        let repo = prepare(&s.a, &s.url, "main").unwrap();
        commit_all(&repo, &Signature::now("t", "t@example.com").unwrap(), "laptop").unwrap();
        let pdf = Oid::hash_file(git2::ObjectType::Blob, s.a.join("Paper.pdf")).unwrap();

        let rules = crate::ignore::IgnoreRules { extensions: vec!["pdf".into()], paths: vec![] };
        crate::ignore::write_rules(&s.a, &rules).unwrap();
        assert!(sync_dev(&s.a, &s.url, "laptop").pushed);

        let server = Repository::open_bare(&s.url).unwrap();
        assert!(server.find_blob(pdf).is_err(), "the excluded PDF must not reach the server");
        sync_dev(&s.b, &s.url, "phone");
        assert!(s.b.join("Note.md").exists());
        assert!(s.a.join("Paper.pdf").exists());
    }

    #[test]
    fn refuses_a_different_existing_remote() {
        let s = setup();
        let repo = Repository::init(&s.a).unwrap();
        repo.remote(REMOTE, "https://example.com/other.git").unwrap();
        assert!(matches!(prepare(&s.a, &s.url, "main"), Err(Error::Setup(_))));
    }
}
