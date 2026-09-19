//! Issue ordering is what keeps `Catalog::revision` and every diagnostics diff stable.

use super::CatalogIssue;
use crate::fs::knowledge::types::INDEX_FILENAME;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub(super) fn compare_issues(left: &CatalogIssue, right: &CatalogIssue) -> Ordering {
    issue_file(left)
        .cmp(issue_file(right))
        .then_with(|| issue_kind(left).cmp(&issue_kind(right)))
        .then_with(|| issue_payload(left).cmp(&issue_payload(right)))
}

/// The issue's identity as one line of a check baseline: kind name, file and
/// payload, with the measured counts (occurrences, lines, bytes) left out so
/// that a recorded issue stays recorded while its size moves.
pub(super) fn baseline_key(issue: &CatalogIssue) -> String {
    let file = issue_file(issue).to_string_lossy().replace('\\', "/");
    let detail = match issue {
        CatalogIssue::DuplicateHeading { heading, .. }
        | CatalogIssue::OversizedSection { heading, .. } => heading.clone(),
        CatalogIssue::OversizedFile { .. } | CatalogIssue::OversizedIndex { .. } => String::new(),
        _ => issue_payload(issue),
    };
    let key = format!("{} {file} {detail}", issue_kind_name(issue));
    key.trim_end().to_string()
}

fn issue_file(issue: &CatalogIssue) -> &Path {
    match issue {
        CatalogIssue::DuplicateHeading { file, .. }
        | CatalogIssue::GenericBlurb { file, .. }
        | CatalogIssue::BrokenLink { file, .. }
        | CatalogIssue::MissingSourceRef { file, .. }
        | CatalogIssue::EvidenceChanged { file, .. }
        | CatalogIssue::EvidenceUnavailable { file, .. }
        | CatalogIssue::UnverifiableReference { file, .. }
        | CatalogIssue::OversizedSection { file, .. }
        | CatalogIssue::OversizedFile { file, .. } => file,
        CatalogIssue::OversizedIndex { .. } => Path::new(INDEX_FILENAME),
        CatalogIssue::DuplicateHeadingAcrossFiles { files, .. } => {
            files.first().map_or(Path::new(""), PathBuf::as_path)
        }
    }
}

fn issue_kind(issue: &CatalogIssue) -> u8 {
    match issue {
        CatalogIssue::DuplicateHeading { .. } => 0,
        CatalogIssue::GenericBlurb { .. } => 1,
        CatalogIssue::BrokenLink { .. } => 2,
        CatalogIssue::MissingSourceRef { .. } => 3,
        CatalogIssue::EvidenceChanged { .. } => 4,
        CatalogIssue::EvidenceUnavailable { .. } => 5,
        CatalogIssue::UnverifiableReference { .. } => 6,
        CatalogIssue::OversizedSection { .. } => 7,
        CatalogIssue::OversizedFile { .. } => 8,
        CatalogIssue::OversizedIndex { .. } => 9,
        CatalogIssue::DuplicateHeadingAcrossFiles { .. } => 10,
    }
}

fn issue_kind_name(issue: &CatalogIssue) -> &'static str {
    match issue {
        CatalogIssue::DuplicateHeading { .. } => "duplicate-heading",
        CatalogIssue::GenericBlurb { .. } => "generic-blurb",
        CatalogIssue::BrokenLink { .. } => "broken-link",
        CatalogIssue::MissingSourceRef { .. } => "missing-source-ref",
        CatalogIssue::EvidenceChanged { .. } => "evidence-changed",
        CatalogIssue::EvidenceUnavailable { .. } => "evidence-unavailable",
        CatalogIssue::UnverifiableReference { .. } => "unverifiable-reference",
        CatalogIssue::OversizedSection { .. } => "oversized-section",
        CatalogIssue::OversizedFile { .. } => "oversized-file",
        CatalogIssue::OversizedIndex { .. } => "oversized-index",
        CatalogIssue::DuplicateHeadingAcrossFiles { .. } => "duplicate-heading-across-files",
    }
}

fn issue_payload(issue: &CatalogIssue) -> String {
    match issue {
        CatalogIssue::DuplicateHeading {
            heading,
            occurrences,
            ..
        } => format!("{heading}:{occurrences}"),
        CatalogIssue::GenericBlurb { blurb, .. } => blurb.clone(),
        CatalogIssue::BrokenLink { target, .. } => target.clone(),
        CatalogIssue::MissingSourceRef { source_path, .. } => source_path.clone(),
        CatalogIssue::EvidenceChanged {
            source_path,
            verified,
            ..
        } => format!("{source_path}:{verified}"),
        CatalogIssue::EvidenceUnavailable {
            source_path,
            reason,
            ..
        } => format!("{source_path}:{}", reason.as_str()),
        CatalogIssue::UnverifiableReference {
            source_path, kind, ..
        } => format!("{source_path}:{kind}"),
        CatalogIssue::OversizedSection { heading, lines, .. } => format!("{heading}:{lines}"),
        CatalogIssue::OversizedFile { lines, .. } => lines.to_string(),
        CatalogIssue::OversizedIndex { bytes } => bytes.to_string(),
        CatalogIssue::DuplicateHeadingAcrossFiles { heading, files } => {
            let files: Vec<_> = files.iter().map(|file| file.to_string_lossy()).collect();
            format!("{heading}:{}", files.join(","))
        }
    }
}
