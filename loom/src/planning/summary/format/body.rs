//! Summary body: line classifier, sentence extraction, link scan and canonical render.

use std::collections::HashMap;

use super::{Block, FormatError, Link, LinkTarget, Sentence, SummaryBody};
use crate::context::source_graph::body_hash;

/// Id of a sentence: `s-` plus the first 8 hex digits of the SHA-256 of its
/// whitespace-collapsed text (links already removed by the caller).
pub fn sentence_id(text_without_links: &str) -> String {
    let collapsed = collapse(text_without_links);
    let hash = body_hash(collapsed.as_bytes());
    let hex = hash.strip_prefix("sha256:").unwrap_or(&hash);
    format!("s-{}", &hex[..8])
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A `[label](target)` span found in a line; byte offsets into that line.
struct RawLink {
    label: String,
    target: String,
    start: usize,
    /// Offset of the `(` that opens the target.
    open: usize,
    /// Offset one past the closing `)`.
    end: usize,
}

/// Finds every inline link of `line`; a malformed target is an error message.
fn scan_links(line: &str) -> Result<Vec<RawLink>, String> {
    let bytes = line.as_bytes();
    let pairs = bracket_pairs(bytes);
    // A `)` search from `from` can only succeed while a `)` sits at or after
    // it; checking this first keeps a failing search from rescanning the line.
    let last_paren = line.rfind(')');
    let mut links = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        let close = pairs[i].filter(|c| bytes.get(c + 1) == Some(&b'('));
        let Some(close) = close else {
            i += 1;
            continue;
        };
        let from = close + 2;
        let rel = last_paren
            .filter(|paren| *paren >= from)
            .and_then(|_| line[from..].find(')'));
        let Some(rel) = rel else {
            i += 1;
            continue;
        };
        let end = from + rel;
        let target = &line[from..end];
        if target.is_empty() || target.contains(char::is_whitespace) || target.contains(['<', '('])
        {
            return Err(format!("malformed link target {target:?}"));
        }
        links.push(RawLink {
            label: line[i + 1..close].to_string(),
            target: target.to_string(),
            start: i,
            open: close + 1,
            end: end + 1,
        });
        i = end + 1;
    }
    Ok(links)
}

/// For every `[` of `bytes`, the offset of the `]` that closes it, found in one
/// pass with a stack so a line of unmatched brackets costs linear time. A `]`
/// with nothing open is ignored; an unclosed `[` has no entry.
fn bracket_pairs(bytes: &[u8]) -> Vec<Option<usize>> {
    let mut pairs = vec![None; bytes.len()];
    let mut open = Vec::new();
    for (at, byte) in bytes.iter().enumerate() {
        match byte {
            b'[' => open.push(at),
            b']' => {
                if let Some(start) = open.pop() {
                    pairs[start] = Some(at);
                }
            }
            _ => {}
        }
    }
    pairs
}

/// What one non-rejected line is.
enum Line {
    Blank,
    Heading { level: u8, text: String },
    Sentence { text: String, links: Vec<RawLink> },
}

fn reject_reason(raw: &str) -> Option<&'static str> {
    if raw.starts_with("    ") || raw.starts_with('\t') {
        return Some("indented lines are not allowed");
    }
    let line = raw.trim_end();
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    let numbered =
        digits > 0 && (line[digits..].starts_with(". ") || line[digits..].starts_with(") "));
    if ["- ", "* ", "+ "].iter().any(|p| line.starts_with(p)) || numbered {
        return Some("list items are not allowed");
    }
    if line.starts_with('|') {
        return Some("table rows are not allowed");
    }
    if line.starts_with("```") || line.starts_with("~~~") {
        return Some("code fences are not allowed");
    }
    if line.starts_with('>') {
        return Some("block quotes are not allowed");
    }
    if line.starts_with("###") {
        return Some("headings deeper than level 2 are not allowed");
    }
    None
}

/// True when `<` followed by a letter, `/` or `!` occurs outside link targets.
fn has_html(line: &str, links: &[RawLink]) -> bool {
    let mut masked = String::new();
    let mut at = 0;
    for link in links {
        masked.push_str(&line[at..link.open]);
        at = link.end;
    }
    masked.push_str(&line[at..]);
    let bytes = masked.as_bytes();
    bytes
        .windows(2)
        .any(|w| w[0] == b'<' && (w[1].is_ascii_alphabetic() || w[1] == b'/' || w[1] == b'!'))
}

fn classify(raw: &str) -> Result<Line, String> {
    let line = raw.trim_end();
    if line.trim().is_empty() {
        return Ok(Line::Blank);
    }
    if let Some(reason) = reject_reason(raw) {
        return Err(reason.to_string());
    }
    let links = scan_links(line)?;
    if has_html(line, &links) {
        return Err("HTML is not allowed".to_string());
    }
    if line.starts_with('#') {
        return classify_heading(line, &links);
    }
    sentence_line(line, links)
}

fn classify_heading(line: &str, links: &[RawLink]) -> Result<Line, String> {
    let (level, rest) = if let Some(rest) = line.strip_prefix("## ") {
        (2, rest)
    } else if let Some(rest) = line.strip_prefix("# ") {
        (1, rest)
    } else {
        return Err("malformed heading".to_string());
    };
    if !links.is_empty() {
        return Err("a heading must not contain a link".to_string());
    }
    let text = collapse(rest);
    if text.is_empty() {
        return Err("empty heading".to_string());
    }
    Ok(Line::Heading { level, text })
}

fn sentence_line(line: &str, links: Vec<RawLink>) -> Result<Line, String> {
    // The trailing run: links separated only by whitespace and ending the line.
    let mut cut = line.len();
    let mut trailing = links.len();
    while trailing > 0 {
        let link = &links[trailing - 1];
        if line[link.end..cut].trim().is_empty() {
            cut = link.start;
            trailing -= 1;
        } else {
            break;
        }
    }
    if trailing == links.len() {
        return Err("sentence has no citation link".to_string());
    }
    if trailing > 0 {
        return Err("sentence text must not contain links before its citations".to_string());
    }
    let text = collapse(&line[..cut]);
    if text.is_empty() {
        return Err("sentence has no text".to_string());
    }
    Ok(Line::Sentence { text, links })
}

/// Accumulates blocks while the lines are walked.
struct State<'a> {
    offset: usize,
    limit: usize,
    resolve: &'a dyn Fn(&str) -> LinkTarget,
    title: Option<String>,
    blocks: Vec<Block>,
    paragraph: Vec<Sentence>,
    seen: HashMap<String, usize>,
    count: usize,
    errors: Vec<FormatError>,
}

impl State<'_> {
    fn error(&mut self, line: usize, message: impl Into<String>) {
        self.errors.push(FormatError {
            line,
            message: message.into(),
        });
    }

    fn flush(&mut self) {
        if !self.paragraph.is_empty() {
            self.blocks
                .push(Block::Paragraph(std::mem::take(&mut self.paragraph)));
        }
    }

    fn heading(&mut self, number: usize, level: u8, text: String) {
        self.flush();
        if self.title.is_none() {
            if level == 1 {
                self.title = Some(text);
            } else {
                self.error(number, "the first line must be a `# ` title");
                self.title = Some(String::new());
            }
            return;
        }
        self.blocks.push(Block::Heading {
            level,
            text,
            line: number - 1,
        });
    }

    fn sentence(&mut self, number: usize, text: String, raw: Vec<RawLink>) {
        if self.title.is_none() {
            self.error(number, "the first line must be a `# ` title");
            self.title = Some(String::new());
        }
        let id = sentence_id(&text);
        if let Some(first) = self.seen.get(&id) {
            let message = format!("duplicate sentence {id}, first at line {first}");
            self.error(number, message);
        } else {
            self.seen.insert(id.clone(), number);
        }
        self.count += 1;
        if self.count == self.limit + 1 {
            let message = format!("more than {} sentences", self.limit);
            self.error(number, message);
        }
        let links = raw
            .into_iter()
            .map(|l| Link {
                resolved: (self.resolve)(&l.target),
                label: l.label,
                target: l.target,
            })
            .collect();
        self.paragraph.push(Sentence {
            id,
            text,
            links,
            line: number - 1,
        });
    }
}

/// Parses a body. `offset` is the number of file lines before `text`, so
/// errors carry file line numbers; `limit` is the sentence cap.
pub(super) fn parse_body(
    text: &str,
    offset: usize,
    limit: usize,
    resolve: &dyn Fn(&str) -> LinkTarget,
) -> Result<SummaryBody, Vec<FormatError>> {
    let mut st = State {
        offset,
        limit,
        resolve,
        title: None,
        blocks: Vec::new(),
        paragraph: Vec::new(),
        seen: HashMap::new(),
        count: 0,
        errors: Vec::new(),
    };
    for (index, raw) in text.lines().enumerate() {
        let number = st.offset + index + 1;
        match classify(raw) {
            Err(message) => st.error(number, message),
            Ok(Line::Blank) => st.flush(),
            Ok(Line::Heading { level, text }) => st.heading(number, level, text),
            Ok(Line::Sentence { text, links }) => st.sentence(number, text, links),
        }
    }
    st.flush();
    if st.title.is_none() {
        st.error(offset + 1, "missing `# ` title");
    }
    match (st.errors.is_empty(), st.title) {
        (true, Some(title)) => Ok(SummaryBody {
            title,
            blocks: st.blocks,
        }),
        _ => Err(st.errors),
    }
}

/// Canonical body text: the title, then each block after one blank line, one
/// final newline. Parsing the output and rendering again gives the same text.
pub fn render_body(title: &str, blocks: &[Block]) -> String {
    let mut out = format!("# {title}\n");
    for block in blocks {
        out.push('\n');
        match block {
            Block::Heading { level, text, .. } => {
                out.push_str(&format!("{} {text}\n", "#".repeat(usize::from(*level))));
            }
            Block::Paragraph(sentences) => {
                for sentence in sentences {
                    out.push_str(&sentence.text);
                    for link in &sentence.links {
                        out.push_str(&format!(" [{}]({})", link.label, link.target));
                    }
                    out.push('\n');
                }
            }
        }
    }
    out
}
