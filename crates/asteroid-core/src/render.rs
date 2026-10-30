//! Markdown to HTML for the reading view.
//!
//! Internal links become `<a class="internal-link" data-href="...">` and
//! attachment embeds become `<img data-vault-src="...">`; the UI turns those
//! into navigation and blob URLs. Note embeds (`![[Note]]`, `![[Note#Heading]]`,
//! `![[Note#^block]]`) are rendered inline, a few levels deep. Output is
//! sanitized because notes may contain arbitrary HTML.

use pulldown_cmark::{CowStr, Event, LinkType, Options, Parser, Tag, TagEnd};

use crate::parse::{self, percent_decode};
use crate::vault::{self, EntryKind};

/// How deep embeds may nest (a note embedding a note embedding a note).
const MAX_EMBED_DEPTH: usize = 3;

/// What rendering needs from the vault: link resolution and other notes' text.
pub trait Links {
    fn resolve(&self, target: &str, from: &str) -> Option<String>;
    fn note_text(&self, path: &str) -> Option<String>;
}

impl Links for crate::Index {
    fn resolve(&self, target: &str, from: &str) -> Option<String> {
        crate::Index::resolve(self, target, from)
    }
    fn note_text(&self, path: &str) -> Option<String> {
        crate::Index::note_text(self, path).map(String::from)
    }
}

/// Renders `text`, a note located at `from`, to sanitized HTML.
pub fn render(text: &str, from: &str, links: &impl Links) -> String {
    let mut stack = vec![from.to_string()];
    sanitize(&render_body(text, from, links, &mut stack, true))
}

/// The HTML for one embed target (e.g. `Note#Heading`) as seen from `from`,
/// for the editor's live preview. None when the target isn't a note.
pub fn render_embed(target: &str, from: &str, links: &impl Links) -> Option<String> {
    let (name, heading) = split_heading(target);
    let path = links.resolve(&name, from)?;
    if EntryKind::from_path(&path) != EntryKind::Note {
        return None;
    }
    let mut stack = vec![from.to_string()];
    Some(sanitize(&note_embed(&path, heading.as_deref(), links, &mut stack)))
}

fn render_body(text: &str, from: &str, links: &impl Links, stack: &mut Vec<String>, properties: bool) -> String {
    let (yaml, skip) = parse::split_frontmatter(text);
    let body: String = if skip > 0 {
        text.lines().skip(skip).collect::<Vec<_>>().join("\n")
    } else {
        text.to_string()
    };
    let body = strip_block_ids(&body);
    let resolve = |target: &str| links.resolve(target, from);

    let mut html = String::new();
    if let Some(yaml) = yaml.filter(|_| properties) {
        html.push_str(&properties_html(yaml));
    }

    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
        | Options::ENABLE_MATH
        | Options::ENABLE_WIKILINKS;
    let mut parser = Parser::new_ext(&body, options);
    let mut events: Vec<Event> = Vec::new();
    // For each open link: whether we replaced its start tag with raw HTML.
    let mut link_stack: Vec<bool> = Vec::new();

    while let Some(event) = parser.next() {
        match event {
            Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
                match classify(link_type, &dest_url) {
                    Dest::Internal { target, heading } => {
                        let resolved = resolve(&target);
                        let class = if resolved.is_some() { "internal-link" } else { "internal-link is-unresolved" };
                        let href = resolved.unwrap_or(target);
                        events.push(Event::InlineHtml(CowStr::from(format!(
                            "<a class=\"{class}\" data-href=\"{}\" data-heading=\"{}\" href=\"#\">",
                            escape(&href),
                            escape(heading.as_deref().unwrap_or(""))
                        ))));
                        link_stack.push(true);
                    }
                    Dest::External => {
                        events.push(Event::InlineHtml(CowStr::from(format!(
                            "<a class=\"external-link\" href=\"{}\" title=\"{}\">",
                            escape(&dest_url),
                            escape(&title)
                        ))));
                        link_stack.push(true);
                    }
                    Dest::Anchor => {
                        events.push(Event::Start(Tag::Link { link_type, dest_url, title, id }));
                        link_stack.push(false);
                    }
                }
            }
            Event::End(TagEnd::Link) => {
                if link_stack.pop().unwrap_or(false) {
                    events.push(Event::InlineHtml(CowStr::Borrowed("</a>")));
                } else {
                    events.push(Event::End(TagEnd::Link));
                }
            }
            Event::Start(Tag::Image { link_type, dest_url, .. }) => {
                // Collect the alt text up to the matching end tag.
                let mut alt = String::new();
                let mut depth = 1;
                for inner in parser.by_ref() {
                    match inner {
                        Event::Start(Tag::Image { .. }) => depth += 1,
                        Event::End(TagEnd::Image) => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        Event::Text(t) | Event::Code(t) => alt.push_str(&t),
                        _ => {}
                    }
                }
                events.push(Event::InlineHtml(CowStr::from(embed_html(link_type, &dest_url, &alt, from, links, stack))));
            }
            other => events.push(other),
        }
    }

    pulldown_cmark::html::push_html(&mut html, events.into_iter());
    html
}

fn split_heading(target: &str) -> (String, Option<String>) {
    match target.split_once('#') {
        Some((name, heading)) => (name.trim().to_string(), Some(heading.trim().to_string()).filter(|h| !h.is_empty())),
        None => (target.trim().to_string(), None),
    }
}

/// An embedded note (or one section or block of it) with a title bar.
/// Too deep, or a note embedding itself, falls back to a link card.
fn note_embed(path: &str, heading: Option<&str>, links: &impl Links, stack: &mut Vec<String>) -> String {
    let stem = vault::stem_of(path);
    let title = match heading {
        // A section named like the note itself needs no second label.
        Some(h) if !h.eq_ignore_ascii_case(stem) => format!("{stem} › {}", h.trim_start_matches('^')),
        _ => stem.to_string(),
    };
    let card = || {
        format!(
            "<a class=\"internal-link embed-card\" data-href=\"{}\" data-heading=\"{}\" href=\"#\">{}</a>",
            escape(path),
            escape(heading.unwrap_or("")),
            escape(&title)
        )
    };
    if stack.len() > MAX_EMBED_DEPTH || stack.iter().any(|p| p == path) {
        return card();
    }
    let Some(text) = links.note_text(path) else { return card() };
    let part = match heading {
        Some(h) if h.starts_with('^') => block_of(&text, &h[1..]),
        Some(h) => section_of(&text, h),
        None => Some(text),
    };
    let body = match part {
        Some(part) => {
            stack.push(path.to_string());
            let html = render_body(&part, path, links, stack, false);
            stack.pop();
            html
        }
        None => "<p class=\"embed-missing\">This section no longer exists.</p>".to_string(),
    };
    format!(
        "<div class=\"embed-note\"><div class=\"embed-header\"><a class=\"internal-link embed-title\" data-href=\"{}\" data-heading=\"{}\" href=\"#\">{}</a></div><div class=\"embed-body\">{body}</div></div>",
        escape(path),
        escape(heading.unwrap_or("")),
        escape(&title)
    )
}

/// The heading named `heading` and everything under it, up to the next
/// heading of the same or a higher level.
fn section_of(text: &str, heading: &str) -> Option<String> {
    let headings = parse::parse(text).headings;
    let wanted = heading.trim().to_lowercase();
    let i = headings.iter().position(|h| h.text.to_lowercase() == wanted)?;
    let start = headings[i].line;
    let end = headings[i + 1..].iter().find(|h| h.level <= headings[i].level).map(|h| h.line);
    let lines: Vec<&str> = text.lines().collect();
    Some(lines[start..end.unwrap_or(lines.len())].join("\n"))
}

/// The paragraph (or list item) ending with the block id `^id`.
fn block_of(text: &str, id: &str) -> Option<String> {
    let marker = format!("^{id}");
    let lines: Vec<&str> = text.lines().collect();
    let end = lines.iter().position(|l| l.trim_end().ends_with(&marker) && l.trim_end().len() > marker.len())?;
    let is_item = |l: &str| l.trim_start().starts_with(['-', '*', '+']) || l.trim_start().chars().next().is_some_and(|c| c.is_ascii_digit());
    let mut start = end;
    if !is_item(lines[end]) {
        while start > 0 && !lines[start - 1].trim().is_empty() && !is_item(lines[start - 1]) {
            start -= 1;
        }
    }
    Some(lines[start..=end].join("\n"))
}

/// Hides Obsidian block ids (" ^abc123" at the end of a line) in rendered text.
fn strip_block_ids(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut fence = false;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if content.trim_start().starts_with("```") || content.trim_start().starts_with("~~~") {
            fence = !fence;
        }
        let stripped = (!fence)
            .then(|| content.rfind(" ^"))
            .flatten()
            .filter(|&i| {
                let id = &content[i + 2..];
                !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            })
            .map(|i| &content[..i]);
        out.push_str(stripped.unwrap_or(content));
        out.push_str(&line[content.len()..]);
    }
    out
}

enum Dest {
    Internal { target: String, heading: Option<String> },
    External,
    Anchor,
}

fn classify(link_type: LinkType, dest: &str) -> Dest {
    let wiki = matches!(link_type, LinkType::WikiLink { .. });
    if !wiki && dest.starts_with('#') {
        return Dest::Anchor;
    }
    if !wiki && (dest.contains("://") || dest.starts_with("mailto:") || dest.starts_with("tel:")) {
        return Dest::External;
    }
    let dest = if wiki { dest.to_string() } else { percent_decode(dest) };
    let (target, heading) = match dest.split_once('#') {
        Some((t, h)) => (t.to_string(), Some(h.to_string())),
        None => (dest, None),
    };
    Dest::Internal { target, heading }
}

fn embed_html(link_type: LinkType, dest: &str, alt: &str, from: &str, links: &impl Links, stack: &mut Vec<String>) -> String {
    let wiki = matches!(link_type, LinkType::WikiLink { .. });
    if !wiki && (dest.starts_with("http://") || dest.starts_with("https://")) {
        return format!("<img src=\"{}\" alt=\"{}\">", escape(dest), escape(alt));
    }
    let target = if wiki { dest.to_string() } else { percent_decode(dest) };
    let (target, heading) = split_heading(&target);
    // Obsidian uses `![[image.png|300]]` to set a width.
    let width = alt.trim().parse::<u32>().ok();
    let label = if wiki && width.is_none() && !alt.is_empty() && alt != dest { alt.to_string() } else { vault::file_name_of(&target).to_string() };

    let Some(path) = links.resolve(&target, from) else {
        return format!("<span class=\"embed-missing\">{}</span>", escape(&label));
    };
    match EntryKind::from_path(&path) {
        EntryKind::Image => {
            let width = width.map(|w| format!(" width=\"{w}\"")).unwrap_or_default();
            format!("<img class=\"embed-image\" data-vault-src=\"{}\" alt=\"{}\"{width}>", escape(&path), escape(&label))
        }
        EntryKind::Pdf => format!(
            "<a class=\"internal-link embed-card embed-pdf\" data-href=\"{}\" href=\"#\">{}</a>",
            escape(&path),
            escape(&label)
        ),
        EntryKind::Note => note_embed(&path, heading.as_deref(), links, stack),
        _ => format!(
            "<a class=\"internal-link embed-card\" data-href=\"{}\" href=\"#\">{}</a>",
            escape(&path),
            escape(&label)
        ),
    }
}

fn properties_html(yaml: &str) -> String {
    let Ok(serde_json::Value::Object(map)) = serde_yaml_ng::from_str::<serde_json::Value>(yaml) else {
        return String::new();
    };
    if map.is_empty() {
        return String::new();
    }
    let mut html = String::from("<div class=\"properties\">");
    for (key, value) in map {
        let rendered = match value {
            serde_json::Value::Array(items) => items
                .iter()
                .map(|v| format!("<span class=\"chip\">{}</span>", escape(&scalar(v))))
                .collect::<String>(),
            other => escape(&scalar(&other)),
        };
        html.push_str(&format!(
            "<div class=\"property\"><span class=\"property-key\">{}</span><span class=\"property-value\">{rendered}</span></div>",
            escape(&key)
        ));
    }
    html.push_str("</div>");
    html
}

fn scalar(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn sanitize(html: &str) -> String {
    ammonia::Builder::default()
        .add_generic_attributes(["class", "data-href", "data-heading", "data-vault-src"])
        .add_tags(["input"])
        .add_tag_attributes("input", ["type", "checked", "disabled"])
        .link_rel(Some("noopener noreferrer"))
        .clean(html)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny vault: notes by path, resolved by exact path or file stem.
    struct Fake(Vec<(&'static str, &'static str)>);

    impl Links for Fake {
        fn resolve(&self, target: &str, _from: &str) -> Option<String> {
            let found = self.0.iter().find(|(p, _)| *p == target || vault::stem_of(p) == target || vault::file_name_of(p) == target);
            found.map(|(p, _)| p.to_string())
        }
        fn note_text(&self, path: &str) -> Option<String> {
            self.0.iter().find(|(p, _)| *p == path).map(|(_, t)| t.to_string())
        }
    }

    fn fake() -> Fake {
        Fake(vec![("folder/Note.md", "# Intro\nHello\n## Deeper\nMore\n# Other\nNot this"), ("assets/pic.png", "")])
    }

    fn render(text: &str) -> String {
        super::render(text, "Home.md", &fake())
    }

    #[test]
    fn wikilinks_and_embeds() {
        let html = render("[[Note#Intro|see]] [[Missing]] ![[pic.png|200]]");
        assert!(html.contains("class=\"internal-link\" data-href=\"folder/Note.md\" data-heading=\"Intro\""), "{html}");
        assert!(html.contains(">see</a>"), "{html}");
        assert!(html.contains("internal-link is-unresolved"), "{html}");
        assert!(html.contains("data-vault-src=\"assets/pic.png\""), "{html}");
        assert!(html.contains("width=\"200\""), "{html}");
    }

    #[test]
    fn frontmatter_becomes_properties_and_html_is_sanitized() {
        let html = render("---\ntags: [a, b]\n---\n<script>alert(1)</script>\n\n- [x] done");
        assert!(html.contains("class=\"properties\""), "{html}");
        assert!(html.contains("<span class=\"chip\">a</span>"), "{html}");
        assert!(!html.contains("<script"), "{html}");
        assert!(html.contains("type=\"checkbox\""), "{html}");
    }

    #[test]
    fn external_links_keep_href() {
        let html = render("[site](https://example.com)");
        assert!(html.contains("class=\"external-link\" href=\"https://example.com\""), "{html}");
    }

    #[test]
    fn note_embeds_render_inline() {
        let html = render("![[Note]]");
        assert!(html.contains("class=\"embed-note\""), "{html}");
        assert!(html.contains("Not this"), "{html}");

        let html = render("![[Note#Intro]]");
        assert!(html.contains("Note › Intro"), "{html}");
        assert!(html.contains("More"), "subsections are included: {html}");
        assert!(!html.contains("Not this"), "the next same-level heading ends it: {html}");

        let html = render("![[Note#Gone]]");
        assert!(html.contains("no longer exists"), "{html}");
    }

    #[test]
    fn embeds_stop_at_cycles_and_depth() {
        let links = Fake(vec![("A.md", "a ![[B]]"), ("B.md", "b ![[A]]")]);
        let html = super::render("![[A]]", "Home.md", &links);
        assert_eq!(html.matches("class=\"embed-note\"").count(), 2, "{html}");
        assert!(html.contains("embed-card"), "the loop back to A becomes a link: {html}");
    }

    #[test]
    fn block_embeds_and_ids() {
        let links = Fake(vec![("N.md", "first line\nsecond line ^quote\n\nother")]);
        let html = super::render("![[N#^quote]]", "Home.md", &links);
        assert!(html.contains("first line") && html.contains("second line"), "{html}");
        assert!(!html.contains("line ^quote") && !html.contains("other"), "{html}");
        assert!(!super::render("text ^abc-1", "Home.md", &links).contains("^abc"));
    }
}
