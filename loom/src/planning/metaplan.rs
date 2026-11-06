//! Metaplan entries: one per planned plan.

use std::path::Path;

use super::kinds::{classify, DocKind};
use super::sections::{sections_of, Span};
use super::slug::plan_slug;
use super::trace::tokens::{section_refs, SectionRef};
use crate::fs::knowledge::splice::fence_mask;

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
    /// The sections and headings of the entry's first `Covers:` line, deduplicated.
    pub covers: Vec<SectionRef>,
    /// The tokens of that line that are not section tokens, as written.
    pub bad_covers: Vec<String>,
    /// The hash of the entry's span, the stub of its plan.
    pub span_hash: String,
}

/// Whether a repository-relative path names a metaplan.
pub fn is_metaplan(rel: &Path) -> bool {
    matches!(classify(rel), DocKind::Metaplan { .. })
}

/// Extract metaplan entries in heading order, ignoring fenced headings.
pub fn entries(text: &str) -> Vec<MetaplanEntry> {
    let lines: Vec<&str> = text.lines().collect();
    let mask = fence_mask(&lines);
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
            let (covers, bad_covers) = covers_of(&lines, &mask, &heading.span);
            Some(MetaplanEntry {
                heading: heading.title,
                anchor: heading.anchor,
                plan_file,
                slug,
                line: heading.span.start_line,
                covers,
                bad_covers,
                span_hash: heading.span.hash,
            })
        })
        .collect()
}

/// The tokens of the first unfenced `Covers:` line inside `span`, so a later entry's line is
/// never read.
fn covers_of(lines: &[&str], mask: &[bool], span: &Span) -> (Vec<SectionRef>, Vec<String>) {
    let end = span.end_line.min(lines.len());
    let value = (span.start_line..end)
        .filter(|&index| !mask[index])
        .find_map(|index| lines[index].strip_prefix("Covers:"));
    value.map_or_else(|| (Vec::new(), Vec::new()), section_refs)
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
