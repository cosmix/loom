//! One human-readable line per `CatalogIssue`, for `loom knowledge check`.

use crate::context::untrusted::inline_safe;
use crate::fs::knowledge::catalog::size::{self, MAX_INDEX_BYTES};
use crate::fs::knowledge::catalog::CatalogIssue;
use crate::fs::knowledge::types::INDEX_FILENAME;
use colored::Colorize;
use std::path::Path;

pub(super) fn decorated_issue_line(issue: &CatalogIssue) -> String {
    if issue.is_review_only() {
        issue_line(issue)
    } else {
        format!("{} {}", "!".yellow().bold(), issue_line(issue))
    }
}

/// One human-readable line per issue. Matched EXHAUSTIVELY — no `_ =>`
/// catch-all — so a future `CatalogIssue` variant fails to compile here
/// instead of silently printing nothing for it.
///
/// Every untrusted field — `heading`, `blurb`, `target`, `source_path`,
/// `verified`, `kind`, and the file path itself — is routed through
/// [`inline_safe`] before it
/// reaches this line. These values come straight from unvalidated knowledge
/// files: `validate_knowledge_content` (`validation.rs:129`) checks only
/// emptiness and length, not control characters, so a heading or blurb can
/// carry an ANSI escape sequence or a bidi override. This is stdout on an
/// agent-facing surface — the same containment `map/views/mod.rs` applies to
/// graph-derived text (`context/untrusted.rs`'s module doc names both
/// surfaces). Do not strip the flattening back out to "simplify" this.
pub(super) fn issue_line(issue: &CatalogIssue) -> String {
    match issue {
        CatalogIssue::DuplicateHeading {
            file,
            heading,
            occurrences,
        } => format!(
            "{}: heading \"{}\" repeated {occurrences} times",
            safe_path(file),
            inline_safe(heading)
        ),
        CatalogIssue::GenericBlurb { file, blurb } => format!(
            "{}: still has the scaffold blurb \"{}\"",
            safe_path(file),
            inline_safe(blurb)
        ),
        CatalogIssue::BrokenLink { file, target } => format!(
            "{}: link target \"{}\" does not resolve",
            safe_path(file),
            inline_safe(target)
        ),
        CatalogIssue::MissingSourceRef { file, source_path } => format!(
            "{}: source reference \"{}\" does not exist",
            safe_path(file),
            inline_safe(source_path)
        ),
        CatalogIssue::EvidenceChanged { .. }
        | CatalogIssue::EvidenceUnavailable { .. }
        | CatalogIssue::UnverifiableReference { .. }
        | CatalogIssue::DuplicateHeadingAcrossFiles { .. } => review_issue_line(issue),
        CatalogIssue::OversizedSection {
            file,
            heading,
            lines,
        } => size_issue_line(file, Some(&inline_safe(heading)), *lines),
        CatalogIssue::OversizedFile { file, lines } => size_issue_line(file, None, *lines),
        CatalogIssue::OversizedIndex { bytes } => format!(
            "{INDEX_FILENAME} is {bytes} bytes, over the {MAX_INDEX_BYTES}-byte budget - trim it"
        ),
    }
}

fn review_issue_line(issue: &CatalogIssue) -> String {
    match issue {
        CatalogIssue::EvidenceChanged {
            file,
            source_path,
            verified,
        } => format!(
            "review: {}: {} changed since {} — re-verify or `loom knowledge annotate {} --verified HEAD`",
            safe_path(file),
            inline_safe(source_path),
            inline_safe(&verified.chars().take(8).collect::<String>()),
            safe_path(file)
        ),
        CatalogIssue::EvidenceUnavailable {
            file,
            source_path,
            reason,
        } => format!(
            "review: {}: evidence for {} is unavailable ({})",
            safe_path(file),
            inline_safe(source_path),
            reason.as_str()
        ),
        CatalogIssue::UnverifiableReference {
            file,
            source_path,
            kind,
        } => format!(
            "note: {}: unresolved {} reference \"{}\"",
            safe_path(file),
            inline_safe(kind),
            inline_safe(source_path)
        ),
        CatalogIssue::DuplicateHeadingAcrossFiles { heading, files } => format!(
            "note: heading \"{}\" appears in {} files ({}) - keep the topic in one place and link to it",
            inline_safe(heading),
            files.len(),
            files.iter().map(|file| safe_path(file)).collect::<Vec<_>>().join(", ")
        ),
        _ => unreachable!("review_issue_line only accepts review-only issues"),
    }
}

/// Flatten a `CatalogIssue`'s relative file path the same way its content
/// fields are flattened — the path is built from a directory walk over the
/// knowledge tree, so it carries whatever bytes are in the file name on
/// disk, same as any other field named in [`issue_line`]'s doc comment.
fn safe_path(file: &Path) -> String {
    inline_safe(&file.display().to_string())
}

/// Shared phrasing for the size issues. A tier-1 overrun points at CLAUDE.md
/// Rule 12's remedy (spill to a tier-2 topic); a tier-2 overrun means the
/// topic itself needs splitting.
fn size_issue_line(file: &Path, heading: Option<&str>, lines: usize) -> String {
    let name = safe_path(file);
    if size::is_tier_one(file) {
        return match heading {
            Some(heading) => format!(
                "{name}: section \"{heading}\" is {lines} lines - move the detail to a tier-2 topic file (CLAUDE.md Rule 12)"
            ),
            None => format!(
                "{name}: tier-1 file is {lines} lines - split the detail into a tier-2 topic file (CLAUDE.md Rule 12)"
            ),
        };
    }
    match heading {
        Some(heading) => format!(
            "{name}: section \"{heading}\" is {lines} lines, over the {}-line tier-2 section limit - split or trim it",
            size::section_limit(file)
        ),
        None => format!(
            "{name}: tier-2 file is {lines} lines, over the {}-line limit - split it into narrower topics",
            size::file_limit(file)
        ),
    }
}
