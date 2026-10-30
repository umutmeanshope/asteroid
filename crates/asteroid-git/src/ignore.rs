//! The vault's "don't sync" rules, kept in a marked block of `.gitignore` so
//! lines the user wrote themselves are never touched. `.gitignore` is
//! committed, so every device applies the same rules.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Result;

const BEGIN: &str = "# >>> Asteroid: files not synced (edit in Settings › Sync)";
const END: &str = "# <<< Asteroid";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoreRules {
    /// Lower-case extensions without the dot, e.g. "pdf".
    pub extensions: Vec<String>,
    /// Vault-relative paths; folders end with "/".
    pub paths: Vec<String>,
}

impl IgnoreRules {
    /// Whether `rel` falls under these rules (used where no repository exists
    /// yet to ask git, e.g. estimating a vault's size before linking).
    pub fn matches(&self, rel: &str) -> bool {
        let ext = rel.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
        if ext.is_some_and(|e| self.extensions.contains(&e)) {
            return true;
        }
        self.paths.iter().any(|p| if p.ends_with('/') { rel.starts_with(p.as_str()) } else { rel == p })
    }
}

pub fn read_rules(root: &Path) -> IgnoreRules {
    let text = fs::read_to_string(root.join(".gitignore")).unwrap_or_default();
    let mut rules = IgnoreRules::default();
    let mut inside = false;
    for line in text.lines() {
        match line.trim_end() {
            BEGIN => inside = true,
            END => inside = false,
            l if inside && !l.is_empty() && !l.starts_with('#') => {
                if let Some(ext) = parse_extension(l) {
                    rules.extensions.push(ext);
                } else if let Some(path) = l.strip_prefix('/') {
                    rules.paths.push(unescape(path));
                }
            }
            _ => {}
        }
    }
    rules
}

/// Replaces Asteroid's block in `.gitignore` (creating the file if needed);
/// everything outside the block is kept as is. An empty rule set removes the
/// block, and the file too if nothing else is left in it.
pub fn write_rules(root: &Path, rules: &IgnoreRules) -> Result<()> {
    let path = root.join(".gitignore");
    let text = fs::read_to_string(&path).unwrap_or_default();
    let mut outside = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        match line.trim_end() {
            BEGIN => inside = true,
            END => inside = false,
            _ if !inside => outside.push(line),
            _ => {}
        }
    }
    while outside.last().is_some_and(|l| l.trim().is_empty()) {
        outside.pop();
    }

    let mut extensions: Vec<String> = rules.extensions.iter().map(|e| normalize_extension(e)).filter(|e| !e.is_empty()).collect();
    extensions.sort();
    extensions.dedup();
    let mut paths: Vec<&String> = rules.paths.iter().filter(|p| !p.is_empty()).collect();
    paths.sort();
    paths.dedup();

    let mut out = outside.join("\n");
    if !extensions.is_empty() || !paths.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(BEGIN);
        out.push('\n');
        for ext in &extensions {
            out.push_str(&extension_pattern(ext));
            out.push('\n');
        }
        for p in paths {
            out.push('/');
            out.push_str(&escape(p));
            out.push('\n');
        }
        out.push_str(END);
    }
    if out.trim().is_empty() {
        match fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => return Ok(()),
        }
    }
    out.push('\n');
    fs::write(path, out)?;
    Ok(())
}

pub fn normalize_extension(ext: &str) -> String {
    ext.trim().trim_start_matches("*.").trim_start_matches('.').to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

/// "pdf" → "*.[pP][dD][fF]": gitignore matching is case-sensitive on Linux,
/// and files named "Paper.PDF" should be caught too.
fn extension_pattern(ext: &str) -> String {
    let mut out = String::from("*.");
    for c in ext.chars() {
        if c.is_ascii_alphabetic() {
            out.push('[');
            out.push(c.to_ascii_lowercase());
            out.push(c.to_ascii_uppercase());
            out.push(']');
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_extension(pattern: &str) -> Option<String> {
    let mut rest = pattern.strip_prefix("*.")?;
    let mut ext = String::new();
    while let Some(c) = rest.chars().next() {
        if c == '[' {
            let group = rest.get(1..3)?;
            let mut chars = group.chars();
            let (lower, upper) = (chars.next()?, chars.next()?);
            if rest.get(3..4)? != "]" || lower.to_ascii_uppercase() != upper {
                return None;
            }
            ext.push(lower);
            rest = &rest[4..];
        } else if c.is_ascii_alphanumeric() {
            ext.push(c.to_ascii_lowercase());
            rest = &rest[c.len_utf8()..];
        } else {
            return None;
        }
    }
    (!ext.is_empty()).then_some(ext)
}

/// Escapes gitignore's special characters so a path matches literally.
fn escape(path: &str) -> String {
    let mut out = String::new();
    for c in path.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '\\' | '!' | '#') {
            out.push('\\');
        }
        out.push(c);
    }
    // Trailing spaces are ignored by git unless escaped.
    if out.ends_with(' ') {
        out.pop();
        out.push_str("\\ ");
    }
    out
}

fn unescape(pattern: &str) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_keeps_user_lines() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "node_modules/\n# mine\n").unwrap();
        let rules = IgnoreRules {
            extensions: vec!["PDF".into(), ".docx".into(), "pdf".into()],
            paths: vec!["Private/Diary [2026].md".into(), "Scans/".into()],
        };
        write_rules(dir.path(), &rules).unwrap();
        let text = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(text.starts_with("node_modules/\n# mine\n\n# >>> Asteroid"));
        assert!(text.contains("*.[pP][dD][fF]\n"));
        assert!(text.contains("/Private/Diary \\[2026\\].md\n"));

        let read = read_rules(dir.path());
        assert_eq!(read.extensions, vec!["docx", "pdf"]);
        assert_eq!(read.paths, vec!["Private/Diary [2026].md", "Scans/"]);
        assert!(read.matches("papers/A.Pdf"));
        assert!(read.matches("Scans/2026/page.png"));
        assert!(!read.matches("Notes.md"));

        // Clearing the rules leaves only the user's own lines.
        write_rules(dir.path(), &IgnoreRules::default()).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join(".gitignore")).unwrap(), "node_modules/\n# mine\n");
    }

    #[test]
    fn git_honours_the_patterns() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let rules = IgnoreRules { extensions: vec!["pdf".into()], paths: vec!["Private/Diary [2026].md".into(), "Scans/".into()] };
        write_rules(dir.path(), &rules).unwrap();
        for (path, ignored) in [
            ("paper.PDF", true),
            ("a/b/paper.pdf", true),
            ("Private/Diary [2026].md", true),
            ("Private/Diary 2.md", false),
            ("Scans/x.png", true),
            ("Notes.md", false),
        ] {
            assert_eq!(repo.is_path_ignored(path).unwrap(), ignored, "{path}");
        }
    }
}
