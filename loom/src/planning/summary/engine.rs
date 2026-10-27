//! Server-side summary engine driven by watcher batches.
//!
//! Each batch: rewrite the paths of document summaries whose source was renamed with its
//! content unchanged, delete the summaries no expected id owns any more, evaluate, and report
//! which summaries newly went out of date.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::claims::list_claims;
use super::evaluate::{self, cite_key, evaluate, same_source, Evaluation, SummaryState};
use super::format::{self, Block, LinkTarget, SummaryFile, SummaryKind};
use super::store;
use super::write::{delete_summary, read_summary, write_summary_if, WriteOutcome};
use crate::planning::project::Project;
use crate::planning::runtime::RuntimePaths;
use crate::planning::sections::normalized_hash;
use crate::planning::watch::{Batch, OwnWrites};

/// What one batch did and the evaluation after it.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineUpdate {
    /// Every summary evaluated after the rewrites and deletions.
    pub evaluation: Evaluation,
    /// Ids whose paths were rewritten after a same-hash rename of their source.
    pub rewritten: Vec<String>,
    /// Ids whose summary file was deleted because their source is gone.
    pub deleted: Vec<String>,
    /// Ids that became stale, incomplete or missing in this batch.
    pub invalidated: Vec<String>,
}

/// The server's summary engine: keeps the store in step with `doc/` and evaluates it.
#[derive(Debug)]
pub struct SummaryEngine {
    root: PathBuf,
    paths: RuntimePaths,
    own: OwnWrites,
    /// Summary states of the previous batch, to report only new invalidations.
    previous: BTreeMap<String, SummaryState>,
}

impl SummaryEngine {
    /// An engine for the repository at `root`, writing under the store lock of `paths` and
    /// registering its writes in `own` (shared with the watcher).
    pub fn new(root: PathBuf, paths: RuntimePaths, own: OwnWrites) -> Self {
        SummaryEngine {
            root,
            paths,
            own,
            previous: BTreeMap::new(),
        }
    }

    /// Process one watcher batch against `project` (already refreshed with it). The batch's
    /// `renamed` list is only a hint and is not consulted: a summary is rewritten whenever its
    /// `of` is an earlier path of its source and its `file` hash equals the source's, which also
    /// catches a rename made while no server ran. A rewrite or deletion that fails is logged
    /// and retried by the next batch.
    pub fn on_batch(&mut self, project: &Project, _batch: &Batch) -> Result<EngineUpdate> {
        let rewritten = self.rewrite_renamed(project);
        let deleted = self.delete_orphans(project);
        let evaluation = evaluate(project, &list_claims(&self.paths)?);
        let invalidated = evaluation
            .summaries
            .iter()
            .filter(|(id, eval)| {
                let before = self.previous.get(*id).is_some_and(|s| s.is_out_of_date());
                eval.state.is_out_of_date() && !before
            })
            .map(|(id, _)| id.clone())
            .collect();
        self.previous = evaluation
            .summaries
            .iter()
            .map(|(id, e)| (id.clone(), e.state))
            .collect();
        Ok(EngineUpdate {
            evaluation,
            rewritten,
            deleted,
            invalidated,
        })
    }

    fn rewrite_renamed(&self, project: &Project) -> Vec<String> {
        let mut rewritten = Vec::new();
        for id in store::expected_ids(project) {
            match self.rewrite_one(project, &id) {
                Ok(true) => rewritten.push(id),
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(summary = %id, error = %format!("{error:#}"), "summary path rewrite failed");
                }
            }
        }
        rewritten
    }

    /// Rewrite summary `id` when its source moved with unchanged content; true when written.
    fn rewrite_one(&self, project: &Project, id: &str) -> Result<bool> {
        let Some(source) = store::source_of(project, id) else {
            return Ok(false);
        };
        let Some(hash) = project.content_hash(&source) else {
            return Ok(false);
        };
        let Some(text) = read_summary(&self.root, id)? else {
            return Ok(false);
        };
        let Ok(file) = format::parse(&store::summary_path(id), &text) else {
            return Ok(false);
        };
        let front = &file.front;
        let moved = front.kind == SummaryKind::Document
            && Path::new(&front.of) != source
            && same_source(id, &front.of, &source)
            && front.file.as_deref() == Some(hash);
        if !moved {
            return Ok(false);
        }
        let new_text = format::render(&with_source(file, id, &source));
        let expected = normalized_hash(&text);
        let own = Some(&self.own);
        let outcome = write_summary_if(
            &self.root,
            &self.paths,
            id,
            Some(&expected),
            || Ok(new_text),
            own,
        )?;
        Ok(outcome == WriteOutcome::Written)
    }

    fn delete_orphans(&self, project: &Project) -> Vec<String> {
        let mut deleted = Vec::new();
        for id in evaluate::orphans(project) {
            match delete_summary(&self.root, &self.paths, &id) {
                Ok(()) => deleted.push(id),
                Err(error) => {
                    tracing::warn!(summary = %id, error = %format!("{error:#}"), "orphan summary deletion failed");
                }
            }
        }
        deleted
    }
}

/// `file` with every path naming its old source (`of`) moved to `source`: `of`, the `cites`
/// keys and the body links, recomputed relative to summary `id`.
fn with_source(mut file: SummaryFile, id: &str, source: &Path) -> SummaryFile {
    let old = PathBuf::from(&file.front.of);
    let cites = std::mem::take(&mut file.front.cites);
    file.front.cites = cites
        .into_iter()
        .map(|(key, hash)| {
            let (path, anchor) = key.split_once('#').unwrap_or((key.as_str(), ""));
            let key = if Path::new(path) == old {
                cite_key(source, anchor)
            } else {
                key.clone()
            };
            (key, hash)
        })
        .collect();
    file.front.of = source.to_string_lossy().into_owned();
    let base = store::link_from_summary(id, source);
    for block in &mut file.blocks {
        let Block::Paragraph(sentences) = block else {
            continue;
        };
        for link in sentences.iter_mut().flat_map(|s| s.links.iter_mut()) {
            let LinkTarget::Source { path, anchor } = &mut link.resolved else {
                continue;
            };
            if *path == old {
                link.target = if anchor.is_empty() {
                    base.clone()
                } else {
                    format!("{base}#{anchor}")
                };
                *path = source.to_path_buf();
            }
        }
    }
    file
}
