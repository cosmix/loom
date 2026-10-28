//! Open-question counts per document.

use std::collections::BTreeMap;

use crate::planning::kinds::DocKind;
use crate::planning::project::Project;

/// Return counts for eligible documents with an `Open questions` heading.
pub fn open_question_counts(project: &Project) -> BTreeMap<String, usize> {
    project
        .docs
        .iter()
        .filter(|(_, doc)| {
            matches!(
                doc.doc.kind,
                DocKind::ProductSection { .. } | DocKind::Metaplan | DocKind::Plan { .. }
            ) && doc
                .sections
                .headings
                .iter()
                .any(|heading| heading.title == "Open questions")
        })
        .map(|(path, doc)| (path.to_string_lossy().into_owned(), doc.open_questions))
        .collect()
}
