//! In-memory index of a vault: link resolution, backlinks, tags and search.
//!
//! The index keeps each note's text so search needs no disk access. It is
//! rebuilt on vault open and updated incrementally as files change.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::parse::{self, Heading, Link};
use crate::vault::{self, EntryKind, Vault};
use crate::Result;

#[derive(Debug, Clone)]
struct NoteEntry {
    title: String,
    aliases: Vec<String>,
    tags: Vec<String>,
    links: Vec<Link>,
    headings: Vec<Heading>,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NoteSummary {
    pub path: String,
    pub title: String,
    pub kind: EntryKind,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Backlink {
    pub source: String,
    pub title: String,
    pub line: usize,
    pub context: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub score: u32,
    pub line: Option<usize>,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagCount {
    pub tag: String,
    pub count: usize,
}

#[derive(Debug, Default, Clone)]
pub struct Index {
    notes: HashMap<String, NoteEntry>,
    /// Every indexed file (notes and attachments), keyed by lowercase path.
    by_lower_path: HashMap<String, String>,
    /// Lowercase file name -> paths with that name.
    by_name: HashMap<String, Vec<String>>,
    /// Lowercase alias -> note paths declaring it.
    by_alias: HashMap<String, Vec<String>>,
}

impl Index {
    pub fn build(vault: &Vault) -> Result<Self> {
        let mut index = Index::default();
        for file in vault.files()? {
            index.insert_file(vault, &file.path, file.kind);
        }
        Ok(index)
    }

    /// Re-reads one path from disk; removes it when it no longer exists.
    pub fn refresh(&mut self, vault: &Vault, rel: &str) {
        if Vault::is_hidden(rel) {
            return;
        }
        let exists = vault.resolve(rel).map(|p| p.is_file()).unwrap_or(false);
        self.remove(rel);
        if exists {
            self.insert_file(vault, rel, EntryKind::from_path(rel));
        } else if vault.resolve(rel).map(|p| p.is_dir()).unwrap_or(false) {
            // A folder appeared (e.g. moved in); index everything below it.
            if let Ok(files) = vault.files() {
                let prefix = format!("{rel}/");
                for file in files.into_iter().filter(|f| f.path.starts_with(&prefix)) {
                    self.remove(&file.path);
                    self.insert_file(vault, &file.path, file.kind);
                }
            }
        }
    }

    /// Removes a file, or every file below it if it was a folder.
    pub fn remove(&mut self, rel: &str) {
        let prefix = format!("{}/", rel.to_lowercase());
        let doomed: Vec<String> = self
            .by_lower_path
            .iter()
            .filter(|(lower, _)| **lower == rel.to_lowercase() || lower.starts_with(&prefix))
            .map(|(_, path)| path.clone())
            .collect();
        for path in doomed {
            self.remove_one(&path);
        }
    }

    fn insert_file(&mut self, vault: &Vault, rel: &str, kind: EntryKind) {
        if kind == EntryKind::Note {
            let Ok(text) = vault.read_text(rel) else { return };
            self.insert_note(rel, text);
        } else {
            self.insert_path(rel);
        }
    }

    /// Indexes a note from text already in memory (used right after saving).
    pub fn insert_note(&mut self, rel: &str, text: String) {
        if self.notes.contains_key(rel) {
            self.remove_one(rel);
        }
        let parsed = parse::parse(&text);
        for alias in &parsed.aliases {
            self.by_alias.entry(alias.to_lowercase()).or_default().push(rel.to_string());
        }
        self.notes.insert(
            rel.to_string(),
            NoteEntry {
                title: vault::stem_of(rel).to_string(),
                aliases: parsed.aliases,
                tags: parsed.tags,
                links: parsed.links,
                headings: parsed.headings,
                text,
            },
        );
        self.insert_path(rel);
    }

    fn insert_path(&mut self, rel: &str) {
        self.by_lower_path.insert(rel.to_lowercase(), rel.to_string());
        let name = vault::file_name_of(rel).to_lowercase();
        let paths = self.by_name.entry(name).or_default();
        if !paths.iter().any(|p| p == rel) {
            paths.push(rel.to_string());
        }
    }

    fn remove_one(&mut self, rel: &str) {
        if let Some(note) = self.notes.remove(rel) {
            for alias in note.aliases {
                if let Some(paths) = self.by_alias.get_mut(&alias.to_lowercase()) {
                    paths.retain(|p| p != rel);
                }
            }
        }
        self.by_lower_path.remove(&rel.to_lowercase());
        if let Some(paths) = self.by_name.get_mut(&vault::file_name_of(rel).to_lowercase()) {
            paths.retain(|p| p != rel);
        }
    }

    /// A copy with files moved (old path → new path), for checking how links
    /// would resolve after a rename without touching the real index.
    pub fn with_moves(&self, moves: &HashMap<String, String>) -> Index {
        let mut index = self.clone();
        let mut notes = Vec::new();
        for old in moves.keys() {
            if let Some(note) = index.notes.get(old) {
                notes.push((old.clone(), note.clone()));
            }
            index.remove_one(old);
        }
        for (old, new) in moves {
            match notes.iter().position(|(o, _)| o == old) {
                Some(i) => {
                    let (_, mut note) = notes.swap_remove(i);
                    note.title = vault::stem_of(new).to_string();
                    for alias in &note.aliases {
                        index.by_alias.entry(alias.to_lowercase()).or_default().push(new.clone());
                    }
                    index.notes.insert(new.clone(), note);
                    index.insert_path(new);
                }
                None => index.insert_path(new),
            }
        }
        index
    }

    /// Every note's path and text.
    pub fn notes_text(&self) -> impl Iterator<Item = (&str, &str)> {
        self.notes.iter().map(|(path, note)| (path.as_str(), note.text.as_str()))
    }

    /// Every indexed path, notes and attachments.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.by_lower_path.values().map(String::as_str)
    }

    /// An indexed path matching `rel` exactly (case-insensitively, `..`
    /// resolved, `.md` optional), without any name-based fallback.
    pub fn lookup(&self, rel: &str) -> Option<String> {
        let normalized = normalize(&rel.replace('\\', "/"))?.to_lowercase();
        self.by_lower_path.get(&normalized).or_else(|| self.by_lower_path.get(&format!("{normalized}.md"))).cloned()
    }

    /// The indexed text of a note.
    pub fn note_text(&self, rel: &str) -> Option<&str> {
        self.notes.get(rel).map(|n| n.text.as_str())
    }

    /// Resolves a link target the way Obsidian does: exact vault path first,
    /// then relative to the linking note, then by file name (closest match
    /// wins), then by alias.
    pub fn resolve(&self, target: &str, from: &str) -> Option<String> {
        let target = target.trim().trim_start_matches('/').replace('\\', "/");
        if target.is_empty() {
            return self.by_lower_path.get(&from.to_lowercase()).cloned();
        }
        let lower = target.to_lowercase();
        let with_md = format!("{lower}.md");
        let variants = [lower.as_str(), with_md.as_str()];

        for v in variants {
            if let Some(path) = self.by_lower_path.get(v) {
                return Some(path.clone());
            }
        }
        let folder = vault::parent_of(from).to_lowercase();
        for v in variants {
            // Relative to the linking note, including `../` steps.
            if let Some(path) = normalize(&vault::join(&folder, v)).and_then(|p| self.by_lower_path.get(&p)) {
                return Some(path.clone());
            }
        }
        for v in variants {
            let name = vault::file_name_of(v);
            let Some(paths) = self.by_name.get(name) else { continue };
            let suffix = format!("/{v}");
            let mut matches: Vec<&String> = paths
                .iter()
                .filter(|p| {
                    let lp = p.to_lowercase();
                    lp == v || lp.ends_with(&suffix)
                })
                .collect();
            matches.sort_by_key(|p| (vault::parent_of(p).to_lowercase() != folder, p.len()));
            if let Some(best) = matches.first() {
                return Some((*best).clone());
            }
        }
        self.by_alias.get(&lower).and_then(|paths| paths.first().cloned())
    }

    pub fn backlinks(&self, rel: &str) -> Vec<Backlink> {
        let mut out = Vec::new();
        for (source, note) in &self.notes {
            if source == rel {
                continue;
            }
            for link in &note.links {
                if self.resolve(&link.target, source).as_deref() == Some(rel) {
                    let context = note.text.lines().nth(link.line).unwrap_or("").trim();
                    out.push(Backlink {
                        source: source.clone(),
                        title: note.title.clone(),
                        line: link.line,
                        context: truncate(context, 200),
                    });
                }
            }
        }
        out.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()).then(a.line.cmp(&b.line)));
        out
    }

    pub fn headings(&self, rel: &str) -> Vec<Heading> {
        self.notes.get(rel).map(|n| n.headings.clone()).unwrap_or_default()
    }

    /// All indexed files, notes first, for the quick switcher and autocomplete.
    pub fn summaries(&self) -> Vec<NoteSummary> {
        let mut out: Vec<NoteSummary> = self
            .by_lower_path
            .values()
            .map(|path| {
                let kind = EntryKind::from_path(path);
                let note = self.notes.get(path);
                NoteSummary {
                    path: path.clone(),
                    title: note.map(|n| n.title.clone()).unwrap_or_else(|| vault::file_name_of(path).to_string()),
                    kind,
                    aliases: note.map(|n| n.aliases.clone()).unwrap_or_default(),
                }
            })
            .collect();
        out.sort_by_key(|s| s.path.to_lowercase());
        out
    }

    pub fn tags(&self) -> Vec<TagCount> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for note in self.notes.values() {
            for tag in &note.tags {
                *counts.entry(tag.clone()).or_default() += 1;
            }
        }
        counts.into_iter().map(|(tag, count)| TagCount { tag, count }).collect()
    }

    /// Full-text search. Every whitespace-separated term must match the title
    /// or body; `#tag` / `tag:x` terms filter by tag instead.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let mut tag_filters = Vec::new();
        let mut terms = Vec::new();
        for raw in query.split_whitespace() {
            if let Some(tag) = raw.strip_prefix("tag:").or_else(|| raw.strip_prefix('#')) {
                tag_filters.push(tag.trim_start_matches('#').to_lowercase());
            } else {
                terms.push(raw.to_lowercase());
            }
        }
        if terms.is_empty() && tag_filters.is_empty() {
            return Vec::new();
        }

        let mut hits = Vec::new();
        for (path, note) in &self.notes {
            let has_tags = tag_filters.iter().all(|want| {
                note.tags.iter().any(|t| {
                    let t = t.to_lowercase();
                    t == *want || t.starts_with(&format!("{want}/"))
                })
            });
            if !has_tags {
                continue;
            }
            let title = note.title.to_lowercase();
            let body = note.text.to_lowercase();
            if !terms.iter().all(|t| title.contains(t.as_str()) || body.contains(t.as_str())) {
                continue;
            }
            let mut score = 1;
            for term in &terms {
                if title == *term {
                    score += 100;
                } else if title.contains(term.as_str()) {
                    score += 30;
                }
                score += body.matches(term.as_str()).count().min(20) as u32;
            }
            let (line, snippet) = match terms.first() {
                Some(first) => note
                    .text
                    .lines()
                    .enumerate()
                    .find(|(_, l)| l.to_lowercase().contains(first.as_str()))
                    .map(|(i, l)| (Some(i), truncate(l.trim(), 160)))
                    .unwrap_or((None, first_line(&note.text))),
                None => (None, first_line(&note.text)),
            };
            hits.push(SearchHit { path: path.clone(), title: note.title.clone(), score, line, snippet });
        }
        hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
        hits.truncate(limit);
        hits
    }
}

/// Resolves `.` and `..` segments; None if the path climbs out of the vault.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

fn first_line(text: &str) -> String {
    let (_, skip) = parse::split_frontmatter(text);
    let line = text.lines().skip(skip).find(|l| !l.trim().is_empty()).unwrap_or("");
    truncate(line.trim(), 160)
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Vault, Index) {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        for (path, text) in files {
            vault.write_text(path, text).unwrap();
        }
        let index = Index::build(&vault).unwrap();
        (dir, vault, index)
    }

    #[test]
    fn resolves_like_obsidian() {
        let (_d, _v, index) = index_with(&[
            ("Daily/Plan.md", ""),
            ("Work/Plan.md", ""),
            ("Work/Meeting.md", "[[Plan]]"),
            ("Ideas.md", "---\naliases: [Brainstorm]\n---\n"),
            ("assets/pic.png", ""),
        ]);
        assert_eq!(index.resolve("Plan", "Work/Meeting.md").as_deref(), Some("Work/Plan.md"));
        assert_eq!(index.resolve("Daily/Plan", "Work/Meeting.md").as_deref(), Some("Daily/Plan.md"));
        assert_eq!(index.resolve("pic.png", "Ideas.md").as_deref(), Some("assets/pic.png"));
        assert_eq!(index.resolve("brainstorm", "x.md").as_deref(), Some("Ideas.md"));
        assert_eq!(index.resolve("Missing", "x.md"), None);
    }

    #[test]
    fn backlinks_and_search() {
        let (_d, _v, mut index) = index_with(&[
            ("A.md", "Links to [[B]] here #rust"),
            ("B.md", "Plain note about Rust"),
            ("C.md", "see [b](B.md)"),
        ]);
        let sources: Vec<_> = index.backlinks("B.md").into_iter().map(|b| b.source).collect();
        assert_eq!(sources, ["A.md", "C.md"]);

        let hits = index.search("rust", 10);
        assert_eq!(hits.len(), 2);
        let tagged = index.search("#rust", 10);
        assert_eq!(tagged.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(), ["A.md"]);

        index.insert_note("B.md", "changed".into());
        assert_eq!(index.search("rust", 10).len(), 1);
        index.remove("A.md");
        assert_eq!(index.backlinks("B.md").len(), 1);
    }
}
