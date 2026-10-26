//! Metaplan entries: one per planned plan.

use std::path::Path;

use super::kinds::{classify, DocKind};
use super::sections::sections_of;
use super::slug::plan_slug;

/// A metaplan heading naming a plan, or a numbered heading missing its plan name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaplanEntry {
    /// The complete heading text.
    pub heading: String,
    /// The heading's deduplicated GitHub anchor.
    pub anchor: String,
    /// The first recognized backticked plan filename, if present.
    pub plan_file: Option<String>,
    /// The recognized plan's lifecycle-independent slug, if present.
    pub slug: Option<String>,
    /// The heading's zero-based line number.
    pub line: usize,
}

/// Whether a repository-relative path names a metaplan.
pub fn is_metaplan(rel: &Path) -> bool {
    matches!(classify(rel), DocKind::Metaplan)
}

/// Extract metaplan entries in heading order, ignoring fenced headings.
pub fn entries(text: &str) -> Vec<MetaplanEntry> {
    sections_of(text)
        .headings
        .into_iter()
        .filter(|heading| heading.level == 2)
        .filter_map(|heading| {
            let plan = plan_in_heading(&heading.title);
            if plan.is_none()
                && (!numbered(&heading.title)
                    || heading.title.to_ascii_lowercase().contains("no plan file"))
            {
                return None;
            }
            let (plan_file, slug) = match plan {
                Some((file, slug)) => (Some(file), Some(slug)),
                None => (None, None),
            };
            Some(MetaplanEntry {
                heading: heading.title,
                anchor: heading.anchor,
                plan_file,
                slug,
                line: heading.span.start_line,
            })
        })
        .collect()
}

fn plan_in_heading(heading: &str) -> Option<(String, String)> {
    let mut parts = heading.split('`');
    parts.next();
    while let (Some(token), Some(_)) = (parts.next(), parts.next()) {
        if let Some(slug) = plan_slug(token) {
            return Some((token.to_owned(), slug));
        }
    }
    None
}

fn numbered(heading: &str) -> bool {
    let digits = heading.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && heading.as_bytes().get(digits) == Some(&b'.')
}
