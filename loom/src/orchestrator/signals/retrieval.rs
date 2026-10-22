//! Retrieval and delivery glue for a stage's on-spawn knowledge brief.

use std::path::Path;

use crate::context::config::RetrievalConfig;
use crate::context::delivery::plan_key;
use crate::context::local_overlay::OverlayScope;
use crate::context::rank_source::{join_working_dir, normalize_dependency_path};
use crate::context::retrieve::{retrieve_for_stage, StageQuery};
use crate::context::schema::ContextPack;
use crate::fs::knowledge::KnowledgeDir;
use crate::fs::work_dir::WorkDir;
use crate::git::merge::control_paths::{accepted_change_base, changed_paths, AcceptedBase};
use crate::models::stage::{Stage, StageStatus, StageType, WiringCheck};

use super::types::EmbeddedContext;

/// The label the Knowledge Brief's `Selected from:` line carries.
///
/// Deliberately a fixed description of [`StageQuery::build_stage_query_text`]'s INPUT
/// FIELDS rather than the query itself: a real stage's query is its whole
/// description — `EXECUTION PLAN` blocks and all — so echoing it back would
/// re-embed the assignment a second time inside the KV-cached semi-stable
/// section, and any `## ` heading or code fence inside it would restructure
/// the signal document. One line, bounded, whatever the stage's size.
pub(super) const STAGE_QUERY_INPUTS: &str = "this stage's id, type, name, description, working dir, files, artifacts, wiring descriptions and dependencies";

/// Append a trailing newline to `content` only if it does not already end
/// with one. Idempotent — safe to call after any number of appended sections.
pub(super) fn ensure_trailing_newline(content: &mut String) {
    if !content.ends_with('\n') {
        content.push('\n');
    }
}

/// Whether the project's `doc/loom/knowledge/` tree holds no real content.
///
/// The root is resolved through [`WorkDir`] so this answer is about the same
/// tree retrieval itself reads. Any failure to resolve answers `false`: an
/// unproven "your knowledge base is empty" is worse than a missing one, since
/// it sends an agent off to re-document a codebase that is already documented.
pub(super) fn knowledge_tree_is_empty(work_dir: &Path) -> bool {
    let Ok(resolved) = WorkDir::new(work_dir) else {
        return false;
    };
    let Some(project_root) = resolved.project_root() else {
        return false;
    };
    !KnowledgeDir::new(project_root).has_content()
}

/// Retrieve this stage's knowledge brief.
///
/// Degrades to `None` on any failure or empty result: a signal that cannot be
/// written is a stalled stage, whereas a signal without a brief is merely a
/// thinner one.
pub(super) fn retrieve_stage_pack(work_dir: &Path, stage: &Stage) -> Option<ContextPack> {
    let main_root = WorkDir::new(work_dir)
        .ok()
        .and_then(|resolved| resolved.main_project_root())
        .unwrap_or_else(|| work_dir.to_path_buf());
    let config = RetrievalConfig::load(&main_root);
    let mut query = StageQuery::new(work_dir, StageQuery::build_stage_query_text(stage));
    query.overlay = StageQuery::stage_overlay_scope(stage);
    query.stage_dependency_ids = crate::context::delivery::dependency_chunk_ids(
        work_dir,
        plan_key(stage),
        &stage.dependencies,
    );
    query.dependency_paths = StageQuery::dependency_paths(work_dir, stage);
    match retrieve_for_stage(&query, config.stage_brief_budget_tokens) {
        Ok(pack) if !pack.items.is_empty() => Some(pack),
        Ok(_) => None,
        Err(error) => {
            tracing::debug!(
                %error,
                stage_id = %stage.id,
                "stage knowledge brief retrieval failed"
            );
            None
        }
    }
}

/// What one accepted dependency changed, as far as the repository can tell.
#[derive(Debug, Clone)]
pub(super) struct DependencyChange {
    /// The record `verify::transitions::load_stage` read.
    pub(super) stage: Stage,
    /// `(base, completed)` when `paths` came from git.
    pub(super) range: Option<(String, String)>,
    /// Repository-relative, normalized, deduplicated.
    pub(super) paths: Vec<String>,
}

/// Whether a declared entry is a glob; globs never equal a graph node path.
fn is_glob(entry: &str) -> bool {
    entry.contains(['*', '?', '['])
}

/// The files and artifacts `dependency` declared, joined to its `working_dir`
/// so they are repository-relative like graph node paths. Globs are skipped.
fn declared_paths(dependency: &Stage) -> Vec<String> {
    dependency
        .files
        .iter()
        .chain(dependency.artifacts.iter())
        .filter(|entry| !is_glob(entry))
        .filter_map(|entry| join_working_dir(dependency.working_dir.as_deref(), entry))
        .collect()
}

/// `(base, completed)` and the paths the dependency's merge brought to the
/// target, or `None` when git cannot name a change (see [`dependency_change`]).
fn git_change(work_dir: &Path, dependency: &Stage) -> Option<((String, String), Vec<String>)> {
    let completed = dependency.completed_commit.as_ref()?;
    let root = WorkDir::new(work_dir).ok()?.main_project_root()?;
    let target = crate::fs::resolve_target_branch_from_config(work_dir, &root).ok()?;
    let base = match accepted_change_base(&root, completed, &target) {
        Ok(AcceptedBase::Merged { base, .. }) => base,
        Ok(AcceptedBase::Unmerged { base }) if !dependency.merge_assumed => base,
        Ok(AcceptedBase::Unmerged { .. }) => return None,
        Err(error) => {
            tracing::debug!(%error, stage_id = %dependency.id, "no accepted change base");
            return None;
        }
    };
    // The commit already reached the target with no merge of its own (carried
    // in by a downstream branch, or fast-forwarded): the diff would be empty.
    if base.eq_ignore_ascii_case(completed) {
        return None;
    }
    match changed_paths(&root, &base, completed) {
        Ok(paths) => Some(((base, completed.clone()), paths)),
        Err(error) => {
            tracing::debug!(%error, stage_id = %dependency.id, "no changed paths");
            None
        }
    }
}

/// Git path for a completed, non-knowledge dependency with a commit; the
/// declared files otherwise.
fn dependency_change(work_dir: &Path, dependency: Stage) -> DependencyChange {
    let from_git = (dependency.stage_type != StageType::Knowledge
        && dependency.status == StageStatus::Completed)
        .then(|| git_change(work_dir, &dependency))
        .flatten();
    let (range, raw) = match from_git {
        Some((range, paths)) => (Some(range), paths),
        None => (None, declared_paths(&dependency)),
    };
    let mut paths: Vec<String> = Vec::new();
    for path in raw {
        // Normalized through the ranker's own helper so the producer and
        // the exact-string comparison on the other end cannot drift apart.
        let normalized = normalize_dependency_path(&path);
        if !normalized.is_empty() && !paths.contains(&normalized) {
            paths.push(normalized);
        }
    }
    DependencyChange {
        stage: dependency,
        range,
        paths,
    }
}

/// One [`DependencyChange`] per readable dependency of `stage`, in order.
///
/// `work_dir` is the resolved `.loom/work` root. A dependency whose record
/// cannot be read is skipped: fewer boosts only thin the brief.
pub(super) fn dependency_changes(work_dir: &Path, stage: &Stage) -> Vec<DependencyChange> {
    stage
        .dependencies
        .iter()
        .filter_map(|id| crate::verify::transitions::load_stage(id, work_dir).ok())
        .map(|dependency| dependency_change(work_dir, dependency))
        .collect()
}

impl StageQuery {
    /// Repository-relative paths the accepted dependencies of `stage` changed.
    ///
    /// Feeds `RankQuery::dependency_paths`, which boosts a source node whose
    /// file a dependency changed: the files a stage we wait on just rewrote are
    /// the files this stage is most likely to be about. The paths come from the
    /// dependency's merge in git ([`dependency_changes`]) and fall back to its
    /// declared files, joined to its `working_dir`, when git names no change.
    /// Order is kept and duplicates across dependencies are dropped.
    pub(crate) fn dependency_paths(work_dir: &Path, stage: &Stage) -> Vec<String> {
        let mut paths: Vec<String> = Vec::new();
        for change in dependency_changes(work_dir, stage) {
            for path in change.paths {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
        paths
    }

    /// Which source-graph overlay a stage's brief reads: that stage's OWN overlay,
    /// never the checkout-wide [`OverlayScope::Local`] one.
    ///
    /// `Local` would be wrong here: it resolves against the project root of the
    /// `.loom/work/` this query names — the MAIN repository, because signals are generated
    /// by the orchestrator daemon running there — so a stage's brief would describe
    /// the main checkout rather than the worktree the stage is about to edit.
    ///
    /// The plan component MUST be [`plan_key`], and getting it wrong fails SILENTLY.
    /// The overlay this reads is written by `MergeLifecycle::reconcile_overlay`
    /// (`orchestrator/merge_lifecycle.rs`), which keys it by the `plan_id` in
    /// `.loom/work/config.toml`; [`plan_key`] keys it by `Stage::plan_id`, falling back to
    /// `"default"` when the stage names no plan. The two agree because `loom init`
    /// writes both from one parsed plan id (`commands/init/plan_setup.rs`: the config
    /// table and every stage record are stamped from `parsed_plan.id`) — an agreement
    /// pinned end-to-end by `context/tests/overlay_key.rs`, because nothing reports
    /// it breaking: on a mismatch the reader asks for an overlay nobody wrote, and
    /// `GraphStore::resolved` returns the base layer with no error at all. The brief
    /// silently degrades to the last merged revision and every gate still passes.
    ///
    /// One case does still diverge, and normalizing it a second time here would only
    /// hide it: a `.loom/work/config.toml` whose `plan_id` is blank, or a stage record
    /// without one while the config has one. [`plan_key`] resolves both to
    /// `"default"`; the writer normalizes neither, so it files the overlay elsewhere.
    /// The fix is to leave ONE derivation — `MergeLifecycle` keying off
    /// `plan_key_from(config.plan_id())` — which is that module's to make.
    pub(crate) fn stage_overlay_scope(stage: &Stage) -> OverlayScope {
        // Spelled out in full rather than through the `plan_key` import: this is the
        // one derivation that has to match the delivery records', and naming its
        // module here is what makes that agreement visible at the call site instead
        // of only in the doc comment above.
        OverlayScope::Stage {
            plan: crate::context::delivery::plan_key(stage).to_string(),
            stage: stage.id.clone(),
        }
    }

    /// Build the free-text query for a stage's brief from its declared metadata:
    /// id, type, name, description, working directory, files, artifacts,
    /// wiring descriptions, and the ids of its dependencies.
    pub(crate) fn build_stage_query_text(stage: &Stage) -> String {
        let mut parts = vec![
            stage.id.clone(),
            format!("{:?}", stage.stage_type),
            stage.name.clone(),
        ];
        parts.extend(stage.description.clone());
        parts.extend(stage.working_dir.clone());
        parts.extend(stage.files.iter().cloned());
        parts.extend(stage.artifacts.iter().cloned());
        parts.extend(stage.wiring.iter().map(Self::describe_wiring_check));
        parts.extend(stage.dependencies.iter().cloned());
        parts.retain(|part| !part.trim().is_empty());
        parts.join("\n")
    }

    /// Keep only the human description of a wiring check in searchable text.
    fn describe_wiring_check(check: &WiringCheck) -> String {
        check.description.clone()
    }
}

/// Persist a [`crate::context::delivery::DeliveryRecord`] for `session_id`'s
/// pack before the signal file that quotes it is written, so a delivery
/// record never describes a signal that does not exist yet.
///
/// Best-effort by contract: a recording failure never blocks signal
/// generation, and a stage whose signal carries no brief writes nothing.
pub(super) fn persist_delivery(
    work_dir: &Path,
    stage: &Stage,
    session_id: &str,
    embedded_context: &EmbeddedContext,
) {
    let Some(pack) = &embedded_context.context_pack else {
        return;
    };
    let record = crate::context::delivery::DeliveryRecord::from_pack(session_id, pack);
    if let Err(error) =
        crate::context::delivery::record_delivery(work_dir, plan_key(stage), &stage.id, &record)
    {
        tracing::debug!(%error, "failed to persist the context delivery record");
    }
}

#[cfg(test)]
#[path = "retrieval_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "retrieval_change_set_tests.rs"]
mod change_set_tests;
