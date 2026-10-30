//! Keeps links working when files are renamed or moved.
//!
//! Every link in the vault is resolved twice: against the index as it was,
//! and against a copy with the moves applied. Links whose answer changed are
//! rewritten to point at the moved file again; links that still resolve the
//! same way (say, `[[Note]]` after Note moves to another folder) are left
//! exactly as written. The rewritten link keeps its style: a bare name stays a
//! bare name when that's unambiguous, a path stays a path, markdown links stay
//! relative, and headings, aliases and labels are untouched.

use std::collections::HashMap;
use std::ops::Range;

use crate::index::Index;
use crate::parse::{self, percent_decode};
use crate::vault::{self, EntryKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Wiki,
    /// `[label](dest)`; `angle` for `<dest>`, `absolute` for `/dest`.
    Markdown { angle: bool, absolute: bool },
}

/// One link in a note, with the byte range of its target in the text.
#[derive(Debug, Clone)]
struct LinkSpan {
    /// The target as written (percent-decoded for markdown links).
    target: String,
    range: Range<usize>,
    style: Style,
}

/// A note whose text changed because links in it were updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    /// The note's path after the move.
    pub path: String,
    pub text: String,
}

/// Plans the link updates for `moves` (old path → new path, files or
/// folders). Must be called with the index as it was before the move.
pub fn plan(index: &Index, moves: &[(String, String)]) -> Vec<Rewrite> {
    let mapping = expand(index, moves);
    if mapping.is_empty() {
        return Vec::new();
    }
    let after = index.with_moves(&mapping);
    let mut out = Vec::new();
    for (source, text) in index.notes_text() {
        let new_source = mapping.get(source).map(String::as_str).unwrap_or(source);
        let mut edits: Vec<(Range<usize>, String)> = Vec::new();
        for span in link_spans(text) {
            let Some(before) = index.resolve(&span.target, source) else { continue };
            let expected = mapping.get(&before).cloned().unwrap_or(before.clone());
            // Markdown links that were correct paths must stay correct paths
            // (GitHub and other editors read them that way), even when the
            // name alone would still find the file here.
            let strict = matches!(span.style, Style::Markdown { .. })
                && path_target(index, &span, source).as_deref() == Some(before.as_str());
            let now = if strict { path_target(&after, &span, new_source) } else { after.resolve(&span.target, new_source) };
            if now.as_deref() == Some(expected.as_str()) {
                continue;
            }
            edits.push((span.range.clone(), format_target(&span, &expected, new_source, &after)));
        }
        if edits.is_empty() {
            continue;
        }
        let mut new_text = text.to_string();
        edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
        for (range, replacement) in edits {
            new_text.replace_range(range, &replacement);
        }
        out.push(Rewrite { path: new_source.to_string(), text: new_text });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// What a markdown link's path points at when read strictly as a path:
/// relative to the note's folder, or to the vault root with a leading `/`.
fn path_target(index: &Index, span: &LinkSpan, source: &str) -> Option<String> {
    let Style::Markdown { absolute, .. } = span.style else { return None };
    let joined = if absolute { span.target.clone() } else { vault::join(vault::parent_of(source), &span.target) };
    index.lookup(&joined)
}

/// Expands folder moves into one entry per indexed file below them.
fn expand(index: &Index, moves: &[(String, String)]) -> HashMap<String, String> {
    let mut mapping = HashMap::new();
    for (from, to) in moves {
        let prefix = format!("{from}/");
        for path in index.paths() {
            if path == from {
                mapping.insert(path.to_string(), to.clone());
            } else if let Some(rest) = path.strip_prefix(&prefix) {
                mapping.insert(path.to_string(), vault::join(to, rest));
            }
        }
    }
    mapping
}

fn format_target(span: &LinkSpan, expected: &str, source: &str, after: &Index) -> String {
    match span.style {
        Style::Wiki => {
            let keep_md = span.target.to_lowercase().ends_with(".md");
            let strip = |p: &str| -> String {
                match p.strip_suffix(".md") {
                    Some(stem) if !keep_md && EntryKind::from_path(p) == EntryKind::Note => stem.to_string(),
                    _ => p.to_string(),
                }
            };
            if !span.target.contains('/') {
                let name = strip(vault::file_name_of(expected));
                if after.resolve(&name, source).as_deref() == Some(expected) {
                    return name;
                }
            }
            // A full vault path always resolves, by exact match.
            strip(expected)
        }
        Style::Markdown { angle, absolute } => {
            let path = if absolute { format!("/{expected}") } else { relative(vault::parent_of(source), expected) };
            if angle { path } else { encode(&path) }
        }
    }
}

/// `to` relative to the folder `from_dir`, e.g. ("a/b", "a/c/N.md") → "../c/N.md".
fn relative(from_dir: &str, to: &str) -> String {
    let from: Vec<&str> = from_dir.split('/').filter(|s| !s.is_empty()).collect();
    let target: Vec<&str> = to.split('/').collect();
    let common = from.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<&str> = vec![".."; from.len() - common];
    parts.extend(&target[common..]);
    parts.join("/")
}

/// Percent-encodes what would otherwise end or confuse a markdown link.
fn encode(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '%' => out.push_str("%25"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '#' => out.push_str("%23"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            c => out.push(c),
        }
    }
    out
}

/// Every wikilink, embed and relative markdown link in `text`, frontmatter
/// included (Obsidian properties can hold `"[[links]]"`); code is skipped.
fn link_spans(text: &str) -> Vec<LinkSpan> {
    let (_, frontmatter_lines) = parse::split_frontmatter(text);
    let mut spans = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut offset = 0;
    for (line_no, line) in text.split_inclusive('\n').enumerate() {
        let start = offset;
        offset += line.len();
        let line = line.trim_end_matches(['\n', '\r']);
        if line_no >= frontmatter_lines {
            if let Some(marker) = fence_marker(line.trim_start()) {
                match fence {
                    None => fence = Some(marker),
                    Some((ch, len)) if marker.0 == ch && marker.1 >= len => fence = None,
                    Some(_) => {}
                }
                continue;
            }
            if fence.is_some() {
                continue;
            }
        }
        scan_line(line, &mask_inline_code(line), start, &mut spans);
    }
    spans
}

fn scan_line(line: &str, masked: &str, base: usize, out: &mut Vec<LinkSpan>) {
    let bytes = masked.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"[[") {
            if let Some(end) = masked[i + 2..].find("]]") {
                let inner_start = i + 2;
                let inner = &line[inner_start..inner_start + end];
                let target_len = inner.find(['|', '#']).unwrap_or(inner.len());
                let raw = &inner[..target_len];
                let lead = raw.len() - raw.trim_start().len();
                let target = raw.trim();
                if !target.is_empty() {
                    let from = base + inner_start + lead;
                    out.push(LinkSpan { target: target.to_string(), range: from..from + target.len(), style: Style::Wiki });
                }
                i = inner_start + end + 2;
                continue;
            }
        } else if bytes[i] == b'['
            && let Some(close) = masked[i..].find("](")
            && !masked[i + 1..i + close].contains('[')
        {
            let dest_start = i + close + 2;
            if let Some(len) = masked[dest_start..].find(')') {
                let raw = &line[dest_start..dest_start + len];
                let lead = raw.len() - raw.trim_start().len();
                let trimmed = raw.trim();
                let angle = trimmed.starts_with('<') && trimmed.ends_with('>');
                let dest = if angle { &trimmed[1..trimmed.len() - 1] } else { trimmed };
                let path_len = dest.find('#').unwrap_or(dest.len());
                let path = &dest[..path_len];
                let external = dest.contains("://") || dest.starts_with("mailto:") || dest.starts_with("tel:");
                if !path.is_empty() && !external {
                    let from = base + dest_start + lead + usize::from(angle);
                    out.push(LinkSpan {
                        target: percent_decode(path),
                        range: from..from + path.len(),
                        style: Style::Markdown { angle, absolute: path.starts_with('/') },
                    });
                }
                i = dest_start + len + 1;
                continue;
            }
        }
        i += 1;
    }
}

fn fence_marker(line: &str) -> Option<(char, usize)> {
    let ch = line.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = line.chars().take_while(|&c| c == ch).count();
    (len >= 3).then_some((ch, len))
}

/// Blanks out inline code spans byte for byte, so offsets into the result
/// are offsets into `line`.
fn mask_inline_code(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let ticks = bytes[i..].iter().take_while(|&&b| b == b'`').count();
        let closing = "`".repeat(ticks);
        match line[i + ticks..].find(&closing) {
            Some(end) => {
                let stop = i + ticks + end + ticks;
                out[i..stop].fill(b' ');
                i = stop;
            }
            None => i += ticks,
        }
    }
    String::from_utf8(out).expect("only whole code spans are blanked")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vault;

    fn index_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        for (path, text) in files {
            vault.write_text(path, text).unwrap();
        }
        let index = Index::build(&vault).unwrap();
        (dir, index)
    }

    fn rewritten<'a>(plan: &'a [Rewrite], path: &str) -> &'a str {
        &plan.iter().find(|r| r.path == path).unwrap_or_else(|| panic!("{path} not rewritten: {plan:?}")).text
    }

    #[test]
    fn renaming_a_note_updates_every_link_style() {
        let (_d, index) = index_with(&[
            ("Ideas/Draft.md", "# Draft"),
            (
                "Home.md",
                "---\nrelated: \"[[Draft]]\"\n---\nSee [[Draft]], [[Draft#Plan|the plan]], ![[Draft]], [[Ideas/Draft]],\n\
                 [md](Ideas/Draft.md#Plan), [angle](<Ideas/Draft.md>) and `[[Draft]]` in code.\n```\n[[Draft]]\n```\n",
            ),
            ("Ideas/Sub/Deep.md", "[up](../Draft.md)"),
        ]);
        let edits = plan(&index, &[("Ideas/Draft.md".into(), "Ideas/Final Version.md".into())]);
        assert_eq!(
            rewritten(&edits, "Home.md"),
            "---\nrelated: \"[[Final Version]]\"\n---\nSee [[Final Version]], [[Final Version#Plan|the plan]], ![[Final Version]], \
             [[Ideas/Final Version]],\n[md](Ideas/Final%20Version.md#Plan), [angle](<Ideas/Final Version.md>) and `[[Draft]]` in code.\n\
             ```\n[[Draft]]\n```\n"
        );
        assert_eq!(rewritten(&edits, "Ideas/Sub/Deep.md"), "[up](../Final%20Version.md)");
        assert_eq!(edits.len(), 2, "the renamed note itself has no links to fix");
    }

    #[test]
    fn moving_keeps_links_that_still_resolve() {
        let (_d, index) = index_with(&[
            ("Inbox/Note.md", "[[Other]] and [rel](Other.md)"),
            ("Inbox/Other.md", ""),
            ("Home.md", "[[Note]] and [[Inbox/Note]]"),
        ]);
        // Note moves into Archive/: its relative markdown link must follow,
        // the bare [[Note]] and [[Other]] still resolve by name.
        let edits = plan(&index, &[("Inbox/Note.md".into(), "Archive/Note.md".into())]);
        assert_eq!(rewritten(&edits, "Archive/Note.md"), "[[Other]] and [rel](../Inbox/Other.md)");
        assert_eq!(rewritten(&edits, "Home.md"), "[[Note]] and [[Archive/Note]]");
    }

    #[test]
    fn renaming_a_folder_and_ambiguous_names() {
        let (_d, index) = index_with(&[
            ("Old/A.md", ""),
            ("Old/pic.png", ""),
            ("Elsewhere/B.md", ""),
            ("Home.md", "[[Old/A]] ![[pic.png]] [[B]]"),
        ]);
        let edits = plan(&index, &[("Old".into(), "New".into())]);
        assert_eq!(rewritten(&edits, "Home.md"), "[[New/A]] ![[pic.png]] [[B]]");

        // Renaming B to a name that already exists elsewhere needs a path.
        let (_d, index) = index_with(&[("x/Same.md", ""), ("y/B.md", ""), ("Home.md", "[[B]]")]);
        let edits = plan(&index, &[("y/B.md".into(), "y/Same.md".into())]);
        assert_eq!(rewritten(&edits, "Home.md"), "[[y/Same]]");
    }

    #[test]
    fn helpers() {
        assert_eq!(relative("a/b", "a/c/N.md"), "../c/N.md");
        assert_eq!(relative("", "x/N.md"), "x/N.md");
        assert_eq!(relative("x", "x/N.md"), "N.md");
        // Byte for byte: the two-byte "ü" becomes two spaces.
        assert_eq!(mask_inline_code("é `[[ü]]` [[x]]"), format!("é{}[[x]]", " ".repeat(10)));
    }
}
