//! The document model of a project's `doc/` tree and the planning checklist.
//!
//! - `discover`: the no-follow walk of `doc/` and bounded document reads.
//! - `kinds`: document kinds and ids from path and name alone.
//! - `paths`: the shared write-path predicate and the locked set.
//! - `runtime`: the per-project runtime paths.
//! - `sections`: heading sections, anchors and span hashes.
//! - `slug`: plan slugs and lifecycle states.
//! - `watch`: the polling watcher.
//! - `vision`: the vision's `Specification sections` table.
//! - `metaplan`: metaplan entries.
//! - `implements`: plan `Implements:` lines.
//! - `pressure`: pressure-test records and open questions.
//! - `xref`: cross-references between documents.
//! - `briefs`: plan briefs grouped by stage.
//! - `findings`: a feature's findings ledger.
//! - `trace`: a feature's planning trace (edge tokens, settled marks, coverage and gaps).
//! - `problems`: the convention problems `loom plan check` reports.
//! - `project`: the loaded project view.
//! - `checklist`: the planning checklist, current step and offers.

pub mod briefs;
pub mod checklist;
pub mod discover;
pub mod findings;
pub mod guidance;
pub mod implements;
pub mod kinds;
pub mod metaplan;
pub mod paths;
pub mod pressure;
pub mod problems;
pub mod project;
pub mod run_review;
pub mod runtime;
pub mod sections;
pub mod slug;
pub mod summary;
pub mod trace;
pub mod vision;
pub mod watch;
pub mod xref;

#[cfg(test)]
mod tests;
