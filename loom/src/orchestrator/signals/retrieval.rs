//! Retrieval and delivery glue for a stage's on-spawn knowledge brief.
//!
//! Split out of `helpers.rs` (which stayed the pre-existing signal-assembly
//! toolbox) so both files stay under the maintainability line limit. The
//! entry points here are re-exported from `helpers` so existing call sites
//! (`super::helpers::persist_delivery`, etc.) keep working unchanged.

use std::path::Path;

use crate::context::delivery::plan_key;
use crate::context::retrieve::{retrieve_for_stage, StageQuery};
use crate::context::schema::ContextPack;
use crate::fs::knowledge::KnowledgeDir;
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{Stage, WiringCheck};

use super::types::EmbeddedContext;

/// The label the Knowledge Brief's `Selected from:` line carries.
///
/// Deliberately a fixed description of [`build_stage_query_text`]'s INPUT
/// FIELDS rather than the query itself: a real stage's query is its whole
/// description — `EXECUTION PLAN` blocks and all — so echoing it back would
/// re-embed the assignment a second time inside the KV-cached semi-stable
/// section, and any `## ` heading or code fence inside it would restructure
/// the signal document. One line, bounded, whatever the stage's size.
pub(super) const STAGE_QUERY_INPUTS: &str = "this stage's id, type, name, description, working dir, files, artifacts, acceptance criteria, wiring checks and dependencies";

/// Append a trailing newline to `content` only if it does not already end
/// with one. Idempotent — safe to call after any number of appended sections.
pub(super) fn ensure_trailing_newline(content: &mut String) {
    if !content.ends_with('\n') {
        content.push('\n');
    }
}

/// Token budget for a stage's on-spawn knowledge brief.
///
/// An estimate-based ceiling on how much retrieved prose gets quoted inline in
/// a signal — never a saving: retrieval is free to select less than this, and
/// this only caps how much of what it selects gets embedded verbatim.
const STAGE_BRIEF_BUDGET_TOKENS: usize = 3000;

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
    let mut query = StageQuery::new(work_dir, build_stage_query_text(stage));
    query.stage_dependency_ids = crate::context::delivery::dependency_chunk_ids(
        work_dir,
        plan_key(stage),
        &stage.dependencies,
    );
    match retrieve_for_stage(&query, STAGE_BRIEF_BUDGET_TOKENS) {
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

/// Build the free-text query for a stage's brief from its declared metadata:
/// id, type, name, description, working directory, files, artifacts,
/// acceptance commands, wiring checks, and the ids of its dependencies.
fn build_stage_query_text(stage: &Stage) -> String {
    let mut parts = vec![
        stage.id.clone(),
        format!("{:?}", stage.stage_type),
        stage.name.clone(),
    ];
    parts.extend(stage.description.clone());
    parts.extend(stage.working_dir.clone());
    parts.extend(stage.files.iter().cloned());
    parts.extend(stage.artifacts.iter().cloned());
    parts.extend(
        stage
            .acceptance
            .iter()
            .map(|criterion| criterion.command().to_string()),
    );
    parts.extend(stage.wiring.iter().map(describe_wiring_check));
    parts.extend(stage.dependencies.iter().cloned());
    parts.retain(|part| !part.trim().is_empty());
    parts.join("\n")
}

/// Render one wiring check as a single line of searchable text.
fn describe_wiring_check(check: &WiringCheck) -> String {
    format!("{} {} {}", check.source, check.pattern, check.description)
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
mod tests {
    use super::*;

    #[test]
    fn stage_query_inputs_is_a_short_one_line_label() {
        // The whole point of the label is that it cannot grow with the stage.
        assert!(!STAGE_QUERY_INPUTS.contains('\n'));
        assert!(
            STAGE_QUERY_INPUTS.len() < 200,
            "the `Selected from:` label must stay bounded, got {} chars",
            STAGE_QUERY_INPUTS.len()
        );
    }

    #[test]
    fn a_populated_knowledge_tree_is_not_reported_empty() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(".work")).unwrap();
        std::fs::create_dir_all(root.join("doc/loom/knowledge")).unwrap();
        std::fs::write(
            root.join("doc/loom/knowledge/architecture.md"),
            "# Architecture\n\n## Components\n\nReal content.\n",
        )
        .unwrap();

        assert!(!knowledge_tree_is_empty(&root.join(".work")));
    }

    #[test]
    fn a_project_without_a_knowledge_tree_is_reported_empty() {
        let temp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join(".work")).unwrap();

        assert!(knowledge_tree_is_empty(&temp.path().join(".work")));
    }

    #[test]
    fn test_ensure_trailing_newline_appends_when_missing() {
        let mut content = String::from("no newline yet");
        ensure_trailing_newline(&mut content);
        assert_eq!(content, "no newline yet\n");
    }

    #[test]
    fn test_ensure_trailing_newline_is_idempotent() {
        let mut content = String::from("already has one\n");
        ensure_trailing_newline(&mut content);
        assert_eq!(content, "already has one\n");
    }

    #[test]
    fn test_build_stage_query_text_joins_non_empty_fields() {
        let mut stage = Stage::new("My Stage".to_string(), Some("Does a thing".to_string()));
        stage.id = "my-stage".to_string();
        stage.working_dir = Some("crates/foo".to_string());
        stage.files = vec!["src/lib.rs".to_string()];
        stage.dependencies = vec!["dep-1".to_string()];

        let text = build_stage_query_text(&stage);
        assert!(text.contains("my-stage"));
        assert!(text.contains("My Stage"));
        assert!(text.contains("Does a thing"));
        assert!(text.contains("crates/foo"));
        assert!(text.contains("src/lib.rs"));
        assert!(text.contains("dep-1"));
        // No blank lines from unset optional fields (description/working_dir are
        // both set here, but artifacts/acceptance/wiring are empty and must not
        // contribute stray blank entries).
        assert!(!text.lines().any(|line| line.trim().is_empty()));
    }
}
