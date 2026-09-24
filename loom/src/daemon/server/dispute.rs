//! Server-side handler for `Request::DisputeCriteria`.
//!
//! Trust boundary: the daemon owns dispute persistence. Agents
//! attest to failures by sending the RPC; only the daemon writes
//! `.loom/work/disputes/<stage>/<n>/request.md`, only the daemon
//! transitions the stage to `NeedsAdjudication`, and only the
//! daemon writes `verdict.md` and `applied.marker` (the latter two
//! land in a follow-on stage). The handler:
//!
//! 1. acquires a flock on `.loom/work/disputes/<stage>/.lock`
//! 2. validates `criterion_index < stage.acceptance.len()`
//! 3. refuses if `stage.dispute_budget_exhausted()`
//! 4. allocates the next sequential id (`max(existing) + 1`,
//!    starting at 1)
//! 5. creates `.loom/work/disputes/<stage>/<n>/` and writes `request.md`
//!    via the existing `safe_create_new_in_workdir` helper
//! 6. increments `dispute_count`, resets `evidence_rounds = 0`,
//!    transitions the stage to `NeedsAdjudication`, saves the stage
//! 7. responds `Response::DisputeCreated { id }`
//!
//! See `models/dispute.rs` for the on-disk schema; `dispute_store.rs` holds
//! the lock, id allocation, `request.md` write and escalation every dispute
//! kind shares, and `dispute_kinds.rs` files the plan v2 kinds.

use anyhow::Result;
use chrono::Utc;
use std::path::Path;

use super::dispute_store::{escalate_to_human_review, lock_stage_disputes, write_request};
use crate::daemon::protocol::Response;
use crate::models::dispute::{DisputeKind, DisputeRequest};
use crate::verify::transitions::{load_stage, update_stage};

const FAILURE_OUTPUT_MAX_BYTES: usize = 4096;

pub fn handle_dispute_criteria(
    work_dir: &Path,
    stage_id: &str,
    criterion_index: usize,
    reason: String,
    evidence_commit: Option<String>,
    failure_output: Option<String>,
) -> Result<Response> {
    // Reject path-traversal or otherwise malformed ids BEFORE any FS
    // touch. The handler runs under the daemon RPC trust boundary, but the
    // stage_id arrives unvalidated from the wire: a string like
    // "../../tmp/x" would otherwise force `create_dir_all` to materialise
    // attacker-controlled directories outside `.loom/work/disputes/`.
    if let Err(e) = crate::validation::validate_id(stage_id) {
        return Ok(Response::Error {
            message: format!("invalid stage_id: {e}"),
        });
    }

    // Per-stage lock — serialises concurrent dispute filings for the
    // same stage (id allocation + state transition).
    let locked = lock_stage_disputes(work_dir, stage_id)?;
    let work_canonical = &locked.work_dir;

    let stage = load_stage(stage_id, work_canonical)?;

    if criterion_index >= stage.acceptance.len() {
        return Ok(Response::Error {
            message: format!(
                "criterion_index {criterion_index} out of range (stage has {} acceptance criteria)",
                stage.acceptance.len()
            ),
        });
    }
    if stage.dispute_budget_exhausted() {
        // The state-machine permits the escalation from both
        // `CompletedWithFailures` (the typical entry point) and
        // `NeedsAdjudication`; if the stage happens to be in some other status
        // we still return the error to the caller and let an operator
        // intervene. The count in the review reason is the fresh on-disk one.
        let count = stage.dispute_count;
        let max = stage.max_disputes_per_stage();
        escalate_to_human_review(stage_id, work_canonical, |s| {
            format!(
                "Dispute budget exhausted ({} of {} disputes filed)",
                s.dispute_count,
                s.max_disputes_per_stage()
            )
        });
        return Ok(Response::Error {
            message: format!("Dispute budget exhausted ({count} disputes filed; max is {max}).",),
        });
    }

    // Truncate failure_output to 4KB on a char boundary (defensive even
    // though the CLI is expected to pre-truncate).
    let failure_output =
        failure_output.map(|s| truncate_to_byte_limit(&s, FAILURE_OUTPUT_MAX_BYTES));

    // Materialise the dispute directory under the next sequential id and
    // write request.md; the id is allocated by `write_request`.
    let dispute_reason = reason.clone();
    let record = DisputeRequest {
        id: 0,
        stage_id: stage_id.to_string(),
        kind: DisputeKind::Criterion { criterion_index },
        reason,
        evidence_commit,
        failure_output,
        fix_attempts_at_dispute: stage.fix_attempts,
        created_at: Utc::now(),
    };
    let id = write_request(&locked.stage_dir, record)?;

    // Update stage state and persist. Re-read under the stages-dir lock and
    // mutate only the dispute-owned fields (`dispute_count`, `evidence_rounds`,
    // status/close_reason via the transition helper) on the fresh on-disk state,
    // so a concurrent orchestrator/CLI write to unrelated fields is not reverted
    // (A-5). `dispute_count` is incremented from the current persisted value.
    update_stage(stage_id, work_canonical, |s| {
        s.dispute_count = s.dispute_count.saturating_add(1);
        s.tally.evidence_rounds = 0;
        // Transition (handles both Executing and CompletedWithFailures via the helper).
        s.try_request_adjudication(Some(dispute_reason))
    })?;

    // The lock releases when `locked` drops.
    drop(locked);

    Ok(Response::DisputeCreated { id })
}

fn truncate_to_byte_limit(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut acc = String::new();
    let mut byte_count = 0;
    for ch in s.chars() {
        let ch_len = ch.len_utf8();
        if byte_count + ch_len > max_bytes {
            break;
        }
        byte_count += ch_len;
        acc.push(ch);
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::work_dir::WorkDir;
    use crate::models::stage::{Stage, StageStatus};
    use crate::plan::schema::AcceptanceCriterion;
    use crate::verify::transitions::{save_stage, update_stage};
    use tempfile::TempDir;

    fn setup(stage_status: StageStatus, acceptance_len: usize) -> (TempDir, std::path::PathBuf) {
        let tmp = TempDir::new().unwrap();
        let wd = WorkDir::new(tmp.path()).unwrap();
        wd.initialize().unwrap();
        let work_path = wd.root().to_path_buf();
        let mut stage = Stage {
            id: "stage-disp".to_string(),
            name: "Disp".to_string(),
            status: stage_status,
            ..Stage::default()
        };
        for i in 0..acceptance_len {
            stage
                .acceptance
                .push(AcceptanceCriterion::Simple(format!("echo {i}")));
        }
        save_stage(&stage, &work_path).unwrap();
        (tmp, work_path)
    }

    #[test]
    fn dispute_persists_request_md_in_per_id_directory() {
        let (_tmp, work_dir) = setup(StageStatus::Executing, 3);
        let resp = handle_dispute_criteria(
            &work_dir,
            "stage-disp",
            1,
            "bad criterion".to_string(),
            None,
            None,
        )
        .unwrap();
        match resp {
            Response::DisputeCreated { id } => {
                let path = work_dir
                    .join("disputes/stage-disp")
                    .join(id.to_string())
                    .join("request.md");
                assert!(path.exists(), "request.md missing at {}", path.display());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn dispute_does_not_create_verdict_md_or_applied_marker() {
        let (_tmp, work_dir) = setup(StageStatus::Executing, 1);
        handle_dispute_criteria(&work_dir, "stage-disp", 0, "x".to_string(), None, None).unwrap();
        let dispute_dir = work_dir.join("disputes/stage-disp/1");
        assert!(!dispute_dir.join("verdict.md").exists());
        assert!(!dispute_dir.join("applied.marker").exists());
    }

    #[test]
    fn dispute_transitions_to_needs_adjudication() {
        let (_tmp, work_dir) = setup(StageStatus::Executing, 1);
        handle_dispute_criteria(&work_dir, "stage-disp", 0, "x".to_string(), None, None).unwrap();
        let stage = crate::verify::transitions::load_stage("stage-disp", &work_dir).unwrap();
        assert_eq!(stage.status, StageStatus::NeedsAdjudication);
    }

    #[test]
    fn dispute_rejects_when_budget_exhausted() {
        let (_tmp, work_dir) = setup(StageStatus::Executing, 1);
        update_stage("stage-disp", &work_dir, |stage| {
            stage.dispute_count = 3;
            Ok(())
        })
        .unwrap();
        let resp = handle_dispute_criteria(&work_dir, "stage-disp", 0, "x".to_string(), None, None)
            .unwrap();
        match resp {
            Response::Error { message } => assert!(message.contains("budget"), "msg: {message}"),
            other => panic!("expected Error, got {other:?}"),
        }
        // Budget-exhausted MUST escalate the stage to NeedsHumanReview so
        // the agent does not loop futilely. (The state-machine allows the
        // direct Executing → NeedsHumanReview transition used here.)
        let after = crate::verify::transitions::load_stage("stage-disp", &work_dir).unwrap();
        assert_eq!(
            after.status,
            StageStatus::NeedsHumanReview,
            "stage must escalate to NeedsHumanReview on budget exhaustion"
        );
        assert!(
            after
                .review_reason
                .as_deref()
                .unwrap_or("")
                .contains("Dispute budget exhausted"),
            "review_reason should mention budget exhaustion; got: {:?}",
            after.review_reason,
        );
    }

    #[test]
    fn dispute_rejects_invalid_criterion_index() {
        let (_tmp, work_dir) = setup(StageStatus::Executing, 2);
        let resp =
            handle_dispute_criteria(&work_dir, "stage-disp", 99, "x".to_string(), None, None)
                .unwrap();
        match resp {
            Response::Error { message } => {
                assert!(message.contains("out of range"), "msg: {message}")
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn dispute_with_failure_output_truncates_at_4kb() {
        let (_tmp, work_dir) = setup(StageStatus::Executing, 1);
        let big = "a".repeat(10_000);
        let resp =
            handle_dispute_criteria(&work_dir, "stage-disp", 0, "x".to_string(), None, Some(big))
                .unwrap();
        let id = match resp {
            Response::DisputeCreated { id } => id,
            other => panic!("got {other:?}"),
        };
        let path = work_dir.join(format!("disputes/stage-disp/{id}/request.md"));
        let content = std::fs::read_to_string(&path).unwrap();
        // The frontmatter contains the failure_output; parse YAML and
        // check the field length is bounded.
        let yaml_chunk = content.split("---").nth(1).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml_chunk).unwrap();
        let fo = parsed["failure_output"].as_str().unwrap();
        assert!(fo.len() <= 4096, "got {}", fo.len());
    }

    #[test]
    fn dispute_works_from_worktree_with_symlinked_work() {
        // Simulate worktree: parent dir holds real .loom/work, worktree dir
        // holds a .loom/work symlink. `.loom` itself stays a real directory
        // on both sides — only the `work` child is a symlink.
        let tmp = TempDir::new().unwrap();
        let main_repo = tmp.path().join("main");
        let worktree = tmp.path().join("worktree");
        std::fs::create_dir_all(&main_repo).unwrap();
        std::fs::create_dir_all(worktree.join(".loom")).unwrap();
        let real_work = main_repo.join(".loom").join("work");
        let wd = WorkDir::new(&main_repo).unwrap();
        wd.initialize().unwrap();
        // Create symlink worktree/.loom/work -> main/.loom/work
        std::os::unix::fs::symlink(&real_work, worktree.join(".loom").join("work")).unwrap();

        let mut stage = Stage {
            id: "stage-sym".to_string(),
            name: "S".to_string(),
            status: StageStatus::Executing,
            ..Stage::default()
        };
        stage
            .acceptance
            .push(AcceptanceCriterion::Simple("x".to_string()));
        save_stage(&stage, &real_work).unwrap();

        let symlinked_work = worktree.join(".loom").join("work");
        let resp =
            handle_dispute_criteria(&symlinked_work, "stage-sym", 0, "y".to_string(), None, None)
                .unwrap();
        match resp {
            Response::DisputeCreated { id } => {
                // The request.md must land in the REAL .loom/work, not the symlink.
                assert!(real_work
                    .join(format!("disputes/stage-sym/{id}/request.md"))
                    .exists());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn dispute_rejects_path_traversal_in_stage_id() {
        // SEC: handle_dispute_criteria takes stage_id straight from the wire
        // and otherwise feeds it to create_dir_all / safe_fs. validate_id must
        // reject path-traversal shapes BEFORE any FS write happens.
        let (_tmp, work_dir) = setup(StageStatus::Executing, 1);
        let evil = "../../tmp/escape";
        let resp =
            handle_dispute_criteria(&work_dir, evil, 0, "x".to_string(), None, None).unwrap();
        match resp {
            Response::Error { message } => {
                assert!(
                    message.contains("invalid stage_id"),
                    "expected invalid stage_id error, got: {message}",
                );
            }
            other => panic!("expected Error, got {other:?}"),
        }
        // Side-effect check: no escape-attempt directory should exist anywhere
        // under disputes_root.
        let disputes_root = work_dir.join("disputes");
        if disputes_root.exists() {
            for entry in std::fs::read_dir(&disputes_root).unwrap() {
                let name = entry.unwrap().file_name();
                let name = name.to_string_lossy();
                assert!(
                    !name.contains("..") && !name.contains('/'),
                    "found suspicious entry under disputes/: {name}",
                );
            }
        }
    }

    #[test]
    fn concurrent_disputes_allocate_distinct_ids_under_flock() {
        let (_tmp, work_dir) = setup(StageStatus::Executing, 5);
        let r1 = handle_dispute_criteria(&work_dir, "stage-disp", 0, "a".to_string(), None, None)
            .unwrap();
        // Model the orchestrator's accept-verdict path before filing again.
        update_stage("stage-disp", &work_dir, |stage| {
            stage.status = StageStatus::Executing;
            Ok(())
        })
        .unwrap();

        let r2 = handle_dispute_criteria(&work_dir, "stage-disp", 1, "b".to_string(), None, None)
            .unwrap();
        let id1 = match r1 {
            Response::DisputeCreated { id } => id,
            other => panic!("{other:?}"),
        };
        let id2 = match r2 {
            Response::DisputeCreated { id } => id,
            other => panic!("{other:?}"),
        };
        assert_ne!(id1, id2, "ids must be distinct");
        assert_eq!(id2, id1 + 1, "second id must be sequential");
    }
}
