use super::super::CatalogIssue;
use serde::Serialize;
use std::fs;
use std::path::Path;

/// JSON-safe accounting for the evidence state of curated knowledge files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceSummary {
    pub status: String,
    pub declared: usize,
    pub current: usize,
    pub changed: usize,
    pub unavailable: usize,
    pub unassessed: usize,
}

/// Return evidence accounting without opening a context store or mutating the
/// knowledge tree. A file is current only when it declares sources and has no
/// changed or unavailable evidence issue.
pub fn evidence_summary(root: &Path, issues: &[CatalogIssue]) -> EvidenceSummary {
    if !root.is_dir() {
        return missing_summary();
    }
    let mut summary = empty_summary();
    for file in super::super::markdown_files(root).unwrap_or_default() {
        let frontmatter = fs::read(root.join(&file))
            .ok()
            .map(|bytes| crate::fs::knowledge::frontmatter::file_frontmatter(&bytes));
        if frontmatter.is_none_or(|metadata| metadata.sources.is_empty()) {
            summary.unassessed += 1;
        } else if file_has_evidence_issue(&file, issues) {
            summary.declared += 1;
        } else {
            summary.declared += 1;
            summary.current += 1;
        }
    }
    add_issue_counts(&mut summary, issues);
    summary.status = evidence_status(&summary).into();
    summary
}

fn missing_summary() -> EvidenceSummary {
    EvidenceSummary {
        status: "missing".into(),
        declared: 0,
        current: 0,
        changed: 0,
        unavailable: 0,
        unassessed: 0,
    }
}

fn empty_summary() -> EvidenceSummary {
    EvidenceSummary {
        status: String::new(),
        declared: 0,
        current: 0,
        changed: 0,
        unavailable: 0,
        unassessed: 0,
    }
}

fn file_has_evidence_issue(file: &Path, issues: &[CatalogIssue]) -> bool {
    issues.iter().any(|issue| {
        matches!(
            issue,
            CatalogIssue::EvidenceChanged {
                file: issue_file,
                ..
            } | CatalogIssue::EvidenceUnavailable {
                file: issue_file,
                ..
            } if issue_file.as_path() == file
        )
    })
}

fn add_issue_counts(summary: &mut EvidenceSummary, issues: &[CatalogIssue]) {
    summary.changed = issues
        .iter()
        .filter(|issue| matches!(issue, CatalogIssue::EvidenceChanged { .. }))
        .count();
    summary.unavailable = issues
        .iter()
        .filter(|issue| matches!(issue, CatalogIssue::EvidenceUnavailable { .. }))
        .count();
}

fn evidence_status(summary: &EvidenceSummary) -> &'static str {
    if summary.unavailable > 0 {
        "unavailable"
    } else if summary.changed > 0 {
        "changed"
    } else if summary.declared == 0 {
        "unassessed"
    } else if summary.unassessed > 0 {
        "partially_assessed"
    } else {
        "current"
    }
}
