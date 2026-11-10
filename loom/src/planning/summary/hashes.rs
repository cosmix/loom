//! The source hashes summaries record and compare, and what each summary is built from.
//!
//! A summary's `file` hash and its `cites` entries are taken over a document's text without its
//! `Settled:` lines (`settle::unsettled`, the normalization the settle body hash reads too). A
//! reader settling, re-settling or clearing a document therefore changes no summary of it, no
//! overview citing it and nothing the rebuild scheduler counts as an edit.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::Path;

use super::evaluate::{bundle_hashes, members_hash, overview_inputs, Evaluation};
use super::store::is_overview;
use crate::planning::briefs::bundle_files;
use crate::planning::project::Project;
use crate::planning::sections::{normalized_hash, DocSections, Span};

/// A project's documents as summaries hash them: a document holding a `Settled:` line reads
/// the sections the project computed without it when it loaded the text (`ProjectDoc::unsettled`),
/// every other document the sections it loaded.
pub struct SourceHashes<'a> {
    project: &'a Project,
}

impl<'a> SourceHashes<'a> {
    pub fn new(project: &'a Project) -> Self {
        SourceHashes { project }
    }

    /// The project the hashes are taken from.
    pub fn project(&self) -> &'a Project {
        self.project
    }

    /// The sections of the readable Markdown document `path`, hashed without its Settled lines;
    /// `None` for an unknown, unreadable or non-Markdown document.
    pub fn sections(&self, path: &Path) -> Option<&'a DocSections> {
        self.project.content_hash(path)?;
        let doc = self.project.docs.get(path)?;
        Some(doc.unsettled.as_ref().unwrap_or(&doc.sections))
    }

    /// The `file` hash a document summary of `path` records.
    pub fn file_hash(&self, path: &Path) -> Option<&str> {
        self.sections(path)
            .map(|sections| sections.file_hash.as_str())
    }

    /// The span `anchor` addresses in `path`, with the hash a citation of it records.
    pub fn span(&self, path: &Path, anchor: &str) -> Option<&Span> {
        self.sections(path)?.find(anchor)
    }
}

/// One hash per evaluated summary over what it is built from: a document summary's source and
/// plan bundle; an overview's raw sources and, per member, the member's inputs and its summary
/// file's sentences. A hash that differs between two evaluations means the summary's sources
/// were edited in between (for an overview also a member resealed); the rebuild scheduler waits
/// for those edits to stop before it rebuilds the summary in the background.
pub fn input_hashes(project: &Project, evaluation: &Evaluation) -> BTreeMap<String, String> {
    let hashes = SourceHashes::new(project);
    let (overviews, documents): (Vec<_>, Vec<_>) = evaluation
        .summaries
        .values()
        .partition(|eval| is_overview(&eval.id));
    let mut inputs = BTreeMap::new();
    for eval in documents {
        let Some(source) = eval.source.as_deref() else {
            continue;
        };
        let mut parts = vec![file_part(&hashes, source)];
        let bundle = bundle_hashes(project, &bundle_files(project, source));
        parts.extend(
            bundle
                .into_iter()
                .map(|(path, hash)| format!("{path} {hash}")),
        );
        inputs.insert(eval.id.clone(), joined(&parts));
    }
    for eval in overviews {
        let Some((members, sources)) = overview_inputs(project, &eval.id) else {
            continue;
        };
        let mut parts: Vec<String> = sources.iter().map(|p| file_part(&hashes, p)).collect();
        for member in members {
            let own = inputs.get(&member).map_or("-", String::as_str);
            let file = evaluation
                .summaries
                .get(&member)
                .and_then(|m| m.file.as_ref());
            let sentences = file.map_or_else(|| "-".to_owned(), members_hash);
            parts.push(format!("{member} {own} {sentences}"));
        }
        inputs.insert(eval.id.clone(), joined(&parts));
    }
    inputs
}

fn file_part(hashes: &SourceHashes<'_>, path: &Path) -> String {
    let hash = hashes.file_hash(path).unwrap_or("-");
    format!("{} {hash}", path.display())
}

fn joined(parts: &[String]) -> String {
    normalized_hash(&parts.join("\n"))
}
