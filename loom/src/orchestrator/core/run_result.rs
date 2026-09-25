//! Read final persisted states, including stages completed outside active sessions.
use super::OrchestratorResult;
use crate::verify::transitions::load_stage;
use crate::{models::stage::StageStatus, plan::ExecutionGraph};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::path::Path;

pub(super) fn collect_result(
    work_dir: &Path,
    graph: &ExecutionGraph,
    spawned: usize,
    started_at: DateTime<Utc>,
) -> Result<OrchestratorResult> {
    let mut result = OrchestratorResult {
        completed_stages: Vec::new(),
        failed_stages: Vec::new(),
        unfinished_stages: Vec::new(),
        needs_handoff: Vec::new(),
        total_sessions_spawned: spawned,
        started_at,
        completed_at: Utc::now(),
    };
    for node in graph.all_nodes() {
        let stage = load_stage(&node.id, work_dir)
            .with_context(|| format!("Failed to read final state of stage '{}'", node.id))?;
        match stage.status {
            StageStatus::Completed => result.completed_stages.push(stage.id),
            StageStatus::Skipped => {}
            StageStatus::NeedsHandoff => result.needs_handoff.push(stage.id),
            // Terminal failures: the run cannot proceed on these without
            // intervention (retry, merge resolution, human review).
            StageStatus::Blocked
            | StageStatus::MergeConflict
            | StageStatus::CompletedWithFailures
            | StageStatus::MergeBlocked
            | StageStatus::NeedsHumanReview => result.failed_stages.push(stage.id),
            // Merely mid-flight: the run stopped (e.g. `loom stop`) before
            // these reached a terminal status, not because anything failed.
            StageStatus::WaitingForDeps
            | StageStatus::Queued
            | StageStatus::Executing
            | StageStatus::WaitingForInput
            | StageStatus::NeedsAdjudication => result.unfinished_stages.push(stage.id),
        }
    }
    result.completed_stages.sort();
    result.failed_stages.sort();
    result.unfinished_stages.sort();
    result.needs_handoff.sort();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::Stage;
    use crate::verify::transitions::serialize_stage_to_markdown;
    use std::fs;
    use tempfile::TempDir;

    fn fixture() -> (TempDir, ExecutionGraph) {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("stages")).unwrap();
        let definition = serde_yaml::from_str("id: test\nname: Test\nworking_dir: .\n").unwrap();
        (dir, ExecutionGraph::build(vec![definition]).unwrap())
    }

    fn write_status(root: &Path, status: StageStatus) {
        let mut stage = Stage::new("Test".into(), None);
        stage.id = "test".into();
        stage.status = status;
        fs::write(
            root.join("stages/test.md"),
            serialize_stage_to_markdown(&stage).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn recovered_block_and_handoff_do_not_poison_final_result() {
        let (dir, graph) = fixture();
        for status in [StageStatus::Blocked, StageStatus::NeedsHandoff] {
            write_status(dir.path(), status);
            assert!(!collect_result(dir.path(), &graph, 1, Utc::now())
                .unwrap()
                .is_success());
            write_status(dir.path(), StageStatus::Completed);
            let result = collect_result(dir.path(), &graph, 2, Utc::now()).unwrap();
            assert!(result.is_success());
            assert_eq!(result.completed_stages, ["test"]);
        }
    }

    #[test]
    fn unfinished_or_unreadable_stage_cannot_report_success() {
        let (dir, graph) = fixture();
        write_status(dir.path(), StageStatus::Queued);
        let result = collect_result(dir.path(), &graph, 0, Utc::now()).unwrap();
        assert!(!result.is_success());
        // A merely mid-flight status is not a failure: it belongs in
        // `unfinished_stages`, not `failed_stages`.
        assert_eq!(result.unfinished_stages, ["test"]);
        assert!(result.failed_stages.is_empty());

        fs::write(dir.path().join("stages/test.md"), "broken").unwrap();
        assert!(collect_result(dir.path(), &graph, 0, Utc::now()).is_err());
    }

    #[test]
    fn only_unfinished_stages_is_not_success() {
        let (dir, graph) = fixture();
        for status in [
            StageStatus::WaitingForDeps,
            StageStatus::Queued,
            StageStatus::Executing,
            StageStatus::WaitingForInput,
            StageStatus::NeedsAdjudication,
        ] {
            let label = format!("{status:?}");
            write_status(dir.path(), status);
            let result = collect_result(dir.path(), &graph, 0, Utc::now()).unwrap();
            assert!(result.failed_stages.is_empty(), "status {label}");
            assert!(result.needs_handoff.is_empty(), "status {label}");
            assert_eq!(result.unfinished_stages, ["test"], "status {label}");
            assert!(!result.is_success(), "status {label}");
        }
    }

    #[test]
    fn terminal_failure_statuses_populate_failed_not_unfinished() {
        let (dir, graph) = fixture();
        for status in [
            StageStatus::Blocked,
            StageStatus::MergeConflict,
            StageStatus::CompletedWithFailures,
            StageStatus::MergeBlocked,
            StageStatus::NeedsHumanReview,
        ] {
            let label = format!("{status:?}");
            write_status(dir.path(), status);
            let result = collect_result(dir.path(), &graph, 0, Utc::now()).unwrap();
            assert_eq!(result.failed_stages, ["test"], "status {label}");
            assert!(result.unfinished_stages.is_empty(), "status {label}");
            assert!(!result.is_success(), "status {label}");
        }
    }
}
