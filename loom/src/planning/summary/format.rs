//! Summary file format: front matter and body parse and render.

mod body;
mod front;
#[cfg(test)]
mod front_tests;
#[cfg(test)]
mod id_tests;
mod levels;
#[cfg(test)]
mod levels_tests;
#[cfg(test)]
mod tests;

use std::fmt;
use std::path::{Path, PathBuf};

use super::store::{self, STORE_DIR};
pub use body::{render_body, sentence_id};
pub use front::{FrontMatter, SummaryKind};
pub use levels::{cites_own, split_levels, Levels};

/// Most sentences a document summary may hold, and the detail level of a plan summary.
pub const MAX_DOCUMENT_SENTENCES: usize = 15;
/// Most sentences the opening level of a plan summary may hold.
pub const MAX_OPENING_SENTENCES: usize = 8;
/// Most sentences a plan summary may hold: its opening level and its detail level.
pub const MAX_PLAN_SENTENCES: usize = MAX_OPENING_SENTENCES + MAX_DOCUMENT_SENTENCES;
/// Most sentences an overview may hold.
pub const MAX_OVERVIEW_SENTENCES: usize = 30;
/// Most sentences a vision summary may hold.
pub const MAX_VISION_SENTENCES: usize = 8;

/// Most sentences the summary `id` may hold. A plan summary's cap covers both of its levels;
/// the seal checks each level against its own cap.
pub fn sentence_limit(id: &str) -> usize {
    if store::is_overview(id) {
        MAX_OVERVIEW_SENTENCES
    } else if store::is_vision(id) {
        MAX_VISION_SENTENCES
    } else if store::is_plan(id) {
        MAX_PLAN_SENTENCES
    } else {
        MAX_DOCUMENT_SENTENCES
    }
}

/// One problem in a summary or draft; `line` is 1-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatError {
    /// 1-based line number in the text that was parsed.
    pub line: usize,
    /// What is wrong with that line.
    pub message: String,
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.line, self.message)
    }
}

impl std::error::Error for FormatError {}

/// What a link points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// A passage of a source document; `anchor` is empty for the preamble.
    Source {
        /// Repository-relative path of the source document.
        path: PathBuf,
        /// Anchor within it, empty for the preamble.
        anchor: String,
    },
    /// A sentence of another summary in the store.
    Sentence {
        /// Id of that summary, for example `feature/product/section_01`.
        summary: String,
        /// Sentence id, `s-` and 8 hex digits.
        id: String,
    },
    /// A target that resolves to neither; carries the href as written.
    Unresolved(String),
}

/// A citation link at the end of a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Text between the brackets.
    pub label: String,
    /// The target exactly as written.
    pub target: String,
    /// The target classified relative to the summary's own location.
    pub resolved: LinkTarget,
}

/// One sentence line of a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sentence {
    /// `s-` plus 8 hex digits of the text hash.
    pub id: String,
    /// Text with links removed and whitespace collapsed.
    pub text: String,
    /// The citation links, in order.
    pub links: Vec<Link>,
    /// 0-based line number in the parsed text.
    pub line: usize,
}

/// A body block after the title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// A `# ` or `## ` heading; `line` is 0-based.
    Heading {
        /// 1 or 2.
        level: u8,
        /// Heading text.
        text: String,
        /// 0-based line number in the parsed text.
        line: usize,
    },
    /// Consecutive sentence lines.
    Paragraph(Vec<Sentence>),
}

/// A body without front matter, as a draft holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryBody {
    /// Text of the `# ` title line.
    pub title: String,
    /// Headings and paragraphs after the title.
    pub blocks: Vec<Block>,
}

/// A sealed summary: front matter plus body.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryFile {
    /// The front matter.
    pub front: FrontMatter,
    /// Text of the `# ` title line.
    pub title: String,
    /// Headings and paragraphs after the title.
    pub blocks: Vec<Block>,
}

fn classify_href(summary_id: &str, href: &str) -> LinkTarget {
    let scheme = href
        .split('/')
        .next()
        .is_some_and(|first| first.contains(':'));
    let unresolved = || LinkTarget::Unresolved(href.to_string());
    if scheme {
        return unresolved();
    }
    let Some((path, fragment)) = store::resolve_link(summary_id, href) else {
        return unresolved();
    };
    let Ok(inside) = path.strip_prefix(STORE_DIR) else {
        return LinkTarget::Source {
            path,
            anchor: fragment.unwrap_or_default(),
        };
    };
    let summary = inside.to_str().and_then(|s| s.strip_suffix(".md"));
    match (summary, fragment) {
        (Some(summary), Some(id))
            if store::validate_id(summary).is_ok() && id.starts_with("s-") =>
        {
            LinkTarget::Sentence {
                summary: summary.to_string(),
                id,
            }
        }
        _ => unresolved(),
    }
}

fn conflict_errors(text: &str) -> Vec<FormatError> {
    let marks = ["<<<<<<< ", "=======", ">>>>>>> "];
    let hits = text.lines().enumerate();
    let hits = hits.filter(|(_, l)| marks.iter().any(|m| l.starts_with(m)));
    hits.map(|(i, _)| FormatError {
        line: i + 1,
        message: "merge conflict marker".into(),
    })
    .collect()
}

fn one_error(line: usize, message: &str) -> Vec<FormatError> {
    vec![FormatError {
        line,
        message: message.to_string(),
    }]
}

/// Valid summary id of a path such as `doc/loom/summaries/overview/feature/product.md`.
fn id_of(summary_rel: &Path) -> Option<String> {
    let inside = summary_rel.strip_prefix(STORE_DIR).ok()?;
    let id = inside.to_str()?.strip_suffix(".md")?;
    store::validate_id(id).ok()?;
    Some(id.to_string())
}

/// Splits `text` into the front matter lines and the number of lines before the body.
fn split_front(text: &str) -> Result<(String, usize), Vec<FormatError>> {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Err(one_error(1, "missing front matter"));
    }
    let mut yaml = Vec::new();
    for (index, line) in lines.enumerate() {
        if line.trim_end() == "---" {
            return Ok((yaml.join("\n"), index + 2));
        }
        yaml.push(line);
    }
    Err(one_error(1, "front matter is not closed by `---`"))
}

/// Parses a sealed summary at repository-relative `summary_rel` (under the
/// store). Links are classified, not validated against their sources.
pub fn parse(summary_rel: &Path, text: &str) -> Result<SummaryFile, Vec<FormatError>> {
    let conflicts = conflict_errors(text);
    if !conflicts.is_empty() {
        return Err(conflicts);
    }
    let Some(id) = id_of(summary_rel) else {
        return Err(one_error(1, "path is not a file of the summary store"));
    };
    let (yaml, body_offset) = split_front(text)?;
    let front = front::parse_front(&yaml, 2)?;
    let limit = sentence_limit(&id);
    let body_text = text
        .lines()
        .skip(body_offset)
        .collect::<Vec<_>>()
        .join("\n");
    let resolve = |href: &str| classify_href(&id, href);
    let body = body::parse_body(&body_text, body_offset, limit, &resolve)?;
    Ok(SummaryFile {
        front,
        title: body.title,
        blocks: body.blocks,
    })
}

/// Parses the draft of summary `id`: a body with no front matter.
pub fn parse_draft(id: &str, text: &str) -> Result<SummaryBody, Vec<FormatError>> {
    store::validate_id(id).map_err(|error| one_error(1, &error.to_string()))?;
    let conflicts = conflict_errors(text);
    if !conflicts.is_empty() {
        return Err(conflicts);
    }
    if text.lines().next().map(str::trim_end) == Some("---") {
        return Err(one_error(
            1,
            "a draft is the body only, without front matter",
        ));
    }
    let resolve = |href: &str| classify_href(id, href);
    body::parse_body(text, 0, sentence_limit(id), &resolve)
}

/// Renders a summary file deterministically: front matter, one blank line,
/// canonical body (see [`render_body`]).
pub fn render(file: &SummaryFile) -> String {
    let front = front::render_front(&file.front);
    format!(
        "{}\n\n{}",
        front.trim_end(),
        render_body(&file.title, &file.blocks)
    )
}
