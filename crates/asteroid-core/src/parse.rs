//! Lightweight extraction of the structure the index needs: frontmatter,
//! wikilinks, markdown links, tags and headings. Code blocks and inline code
//! are skipped so that `[[x]]` inside code is not treated as a link.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    /// Link target without heading or alias, e.g. `Folder/Note`.
    pub target: String,
    /// Optional `#heading` part.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// Whether this is an embed (`![[...]]`).
    pub embed: bool,
    /// Zero-based line number where the link appears.
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

#[derive(Debug, Default, Clone)]
pub struct Parsed {
    pub frontmatter: Option<serde_json::Value>,
    pub links: Vec<Link>,
    pub tags: Vec<String>,
    pub headings: Vec<Heading>,
    pub aliases: Vec<String>,
}

/// Splits leading YAML frontmatter from the body. Returns the YAML source and
/// the number of lines it occupied (including both `---` fences).
pub fn split_frontmatter(text: &str) -> (Option<&str>, usize) {
    let Some(rest) = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n")) else {
        return (None, 0);
    };
    let mut offset = 0;
    for (i, line) in rest.split_inclusive('\n').enumerate() {
        if line.trim_end() == "---" {
            return (Some(&rest[..offset]), i + 2);
        }
        offset += line.len();
    }
    (None, 0)
}

pub fn parse(text: &str) -> Parsed {
    let mut parsed = Parsed::default();
    let (yaml, skip) = split_frontmatter(text);
    if let Some(yaml) = yaml
        && let Ok(value) = serde_yaml_ng::from_str::<serde_json::Value>(yaml) {
            parsed.tags.extend(string_list(value.get("tags")).map(|t| normalize_tag(&t)));
            parsed.aliases.extend(string_list(value.get("aliases")));
            parsed.frontmatter = Some(value);
        }

    let mut fence: Option<(char, usize)> = None;
    for (line_no, line) in text.lines().enumerate().skip(skip) {
        let trimmed = line.trim_start();
        if let Some(marker) = fence_marker(trimmed) {
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
        if let Some(heading) = parse_heading(trimmed, line_no) {
            parsed.headings.push(heading);
        }
        let visible = strip_inline_code(line);
        extract_links(&visible, line_no, &mut parsed.links);
        extract_tags(&visible, &mut parsed.tags);
    }

    parsed.tags.sort();
    parsed.tags.dedup();
    parsed
}

fn string_list(value: Option<&serde_json::Value>) -> impl Iterator<Item = String> {
    let items: Vec<String> = match value {
        Some(serde_json::Value::String(s)) => {
            s.split([',', ' ']).filter(|s| !s.is_empty()).map(str::to_string).collect()
        }
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    items.into_iter()
}

fn normalize_tag(tag: &str) -> String {
    tag.trim().trim_start_matches('#').to_string()
}

fn fence_marker(line: &str) -> Option<(char, usize)> {
    let ch = line.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = line.chars().take_while(|&c| c == ch).count();
    (len >= 3).then_some((ch, len))
}

fn parse_heading(line: &str, line_no: usize) -> Option<Heading> {
    let level = line.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &line[level..];
    if !rest.is_empty() && !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }
    let text = rest.trim().trim_end_matches('#').trim().to_string();
    (!text.is_empty()).then_some(Heading { level: level as u8, text, line: line_no })
}

/// Replaces inline code spans with spaces so positions stay stable.
fn strip_inline_code(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '`' {
            out.push(c);
            continue;
        }
        let mut ticks = 1;
        while chars.peek() == Some(&'`') {
            chars.next();
            ticks += 1;
        }
        // Find the matching run of backticks; if none, keep the text as-is.
        let rest: String = chars.clone().collect();
        let closing = "`".repeat(ticks);
        match rest.find(&closing) {
            Some(end) => {
                let span = rest[..end].chars().count();
                out.push_str(&" ".repeat(ticks * 2 + span));
                for _ in 0..span + ticks {
                    chars.next();
                }
            }
            None => out.push_str(&closing),
        }
    }
    out
}

fn extract_links(line: &str, line_no: usize, out: &mut Vec<Link>) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"[[") {
            let embed = i > 0 && bytes[i - 1] == b'!';
            if let Some(end) = line[i + 2..].find("]]") {
                let inner = &line[i + 2..i + 2 + end];
                if let Some(link) = wikilink(inner, embed, line_no) {
                    out.push(link);
                }
                i += end + 4;
                continue;
            }
        } else if bytes[i] == b'['
            && let Some(link) = markdown_link(&line[i..], line_no) {
                let embed = i > 0 && bytes[i - 1] == b'!';
                out.push(Link { embed, ..link.0 });
                i += link.1;
                continue;
            }
        i += 1;
    }
}

fn wikilink(inner: &str, embed: bool, line: usize) -> Option<Link> {
    let target = inner.split('|').next()?.trim();
    let (target, heading) = match target.split_once('#') {
        Some((t, h)) => (t.trim(), Some(h.trim().to_string())),
        None => (target, None),
    };
    if target.is_empty() && heading.is_none() {
        return None;
    }
    Some(Link { target: target.to_string(), heading, embed, line })
}

/// Parses `[text](dest)` where dest is a relative path (not a URL).
fn markdown_link(s: &str, line: usize) -> Option<(Link, usize)> {
    let close = s.find("](")?;
    if s[1..close].contains('[') {
        return None;
    }
    let rest = &s[close + 2..];
    let end = rest.find(')')?;
    let dest = rest[..end].trim().trim_start_matches('<').trim_end_matches('>');
    if dest.is_empty() || dest.contains("://") || dest.starts_with('#') || dest.starts_with("mailto:") {
        return None;
    }
    let dest = percent_decode(dest);
    let (target, heading) = match dest.split_once('#') {
        Some((t, h)) => (t.to_string(), Some(h.to_string())),
        None => (dest, None),
    };
    Some((Link { target, heading, embed: false, line }, close + 3 + end))
}

pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = |b: u8| (b as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn extract_tags(line: &str, out: &mut Vec<String>) {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let starts_word = i == 0 || chars[i - 1].is_whitespace() || chars[i - 1] == '(';
        if chars[i] == '#' && starts_word {
            let tag: String = chars[i + 1..]
                .iter()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
                .collect();
            let len = tag.chars().count();
            // Pure numbers (`#1`) are not tags, matching Obsidian.
            if len > 0 && tag.chars().any(|c| !c.is_ascii_digit()) {
                out.push(tag.trim_end_matches('/').to_string());
            }
            i += len + 1;
            continue;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_tags_and_aliases() {
        let p = parse("---\ntags: [project, \"#work\"]\naliases: Plan\n---\n# Title\nbody #inline\n");
        assert_eq!(p.tags, ["inline", "project", "work"]);
        assert_eq!(p.aliases, ["Plan"]);
        assert_eq!(p.headings, [Heading { level: 1, text: "Title".into(), line: 4 }]);
    }

    #[test]
    fn links_skip_code() {
        let text = "See [[Note A#Intro|alias]] and ![[pic.png]].\n```\n[[Not a link]]\n```\n`[[inline]]` [md](Folder/B%20C.md)\n";
        let p = parse(text);
        let targets: Vec<_> = p.links.iter().map(|l| (l.target.as_str(), l.embed, l.line)).collect();
        assert_eq!(targets, [("Note A", false, 0), ("pic.png", true, 0), ("Folder/B C.md", false, 4)]);
        assert_eq!(p.links[0].heading.as_deref(), Some("Intro"));
    }

    #[test]
    fn tags_ignore_headings_numbers_and_urls() {
        let p = parse("# Heading\nIssue #12, see #ideas/rust and http://x.y/#frag #todo");
        assert_eq!(p.tags, ["ideas/rust", "todo"]);
    }
}
