//! Heading tallies over the curated tree: repeats inside one file
//! ([`CatalogIssue::DuplicateHeading`]) and the same heading kept in several
//! files ([`CatalogIssue::DuplicateHeadingAcrossFiles`]).

use super::CatalogIssue;
use crate::fs::knowledge::chunker::KnowledgeChunk;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Headings shorter than this are too generic to mean "the same topic".
const MIN_CROSS_FILE_HEADING_CHARS: usize = 12;

/// Normalized anchors of the structural headings topic files share by
/// convention (a mistake entry's "What happened", a "Related files" list).
const SHARED_HEADING_ANCHORS: &[&str] = &[
    "what-happened",
    "related-files",
    "related-topics",
    "open-questions",
    "implementation-notes",
];

#[derive(Default)]
pub(super) struct HeadingTally {
    per_file: BTreeMap<PathBuf, BTreeMap<String, usize>>,
    across_files: BTreeMap<String, (String, BTreeSet<PathBuf>)>,
}

impl HeadingTally {
    /// Count one chunk's heading. The headingless preamble is ignored.
    pub(super) fn record(&mut self, relative_path: &Path, chunk: &KnowledgeChunk) {
        if chunk.heading.is_empty() {
            return;
        }
        *self
            .per_file
            .entry(relative_path.to_path_buf())
            .or_default()
            .entry(chunk.anchor.clone())
            .or_default() += 1;
        if chunk.heading.chars().count() >= MIN_CROSS_FILE_HEADING_CHARS
            && !SHARED_HEADING_ANCHORS.contains(&chunk.anchor.as_str())
        {
            self.across_files
                .entry(chunk.anchor.clone())
                .or_insert_with(|| (chunk.heading.clone(), BTreeSet::new()))
                .1
                .insert(relative_path.to_path_buf());
        }
    }

    /// Turn the tallies into issues. A separate pass, not part of recording:
    /// a duplicate is only knowable once every section has been counted.
    pub(super) fn push_issues(self, issues: &mut Vec<CatalogIssue>) {
        for (file, counts) in self.per_file {
            for (heading, occurrences) in counts {
                if occurrences > 1 {
                    issues.push(CatalogIssue::DuplicateHeading {
                        file: file.clone(),
                        heading,
                        occurrences,
                    });
                }
            }
        }
        for (heading, files) in self.across_files.into_values() {
            if files.len() > 1 {
                issues.push(CatalogIssue::DuplicateHeadingAcrossFiles {
                    heading,
                    files: files.into_iter().collect(),
                });
            }
        }
    }
}
