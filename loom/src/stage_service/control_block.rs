//! Narrow daemon-side stage block transition.
//!
//! `.loom/work/` is read-only from a stage worktree, so an agent that discovers it
//! cannot proceed has no way to write `stages/<id>.md` itself — which left the
//! sanctioned "say why you are stuck" command unusable from the only place it
//! was ever needed. The daemon owns the write for the same reason it owns
//! completion and dispute persistence, and by the time this runs
//! `server/self_service.rs` has already established that the caller owns the
//! stage.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;

use super::Response;
use crate::handoff::load_trusted_session_checkpoint;
use crate::handoff::schema::{CompletionPhase, CriterionResult};
use crate::models::stage::Stage;
use crate::verify::completion::{CONTRACTS_LABEL, IMPACT_TESTS_LABEL};
use crate::verify::transitions::{load_stage, update_stage};

/// A stage agent's `loom stage block`: refused while the stage session's
/// latest signed `loom stage complete` attempt failed a check a dispute can
/// answer: an `acceptance-<i>` criterion, an indexed goal-backward gap
/// (`artifact-<i>`, `wiring-<i>` or `wiring_tests-<i>`), the frozen contracts
/// or the impact-selected tests, otherwise [`handle_block_stage`].
///
/// Each of those has a route that keeps the stage moving: a dispute, judged by
/// an adjudicator and escalated to the operator once the budget is spent. A
/// block there parks the stage as an intentional block that is never retried,
/// so the work waits on an operator for a question the adjudicator could have
/// answered. A failed completion naming none of them (missing sandbox, setup
/// error, a gap no dispute covers) has no dispute route, so the block stays
/// open. Operator blocks and the daemon's own signing block call
/// [`handle_block_stage`] directly.
#[cfg(test)]
pub(crate) fn handle_agent_block_stage(
    work_dir: &Path,
    stage_id: &str,
    reason: &str,
) -> Result<Response> {
    handle_agent_block_for_session(work_dir, stage_id, reason, None)
}

pub(crate) fn handle_agent_block_for_session(
    work_dir: &Path,
    stage_id: &str,
    reason: &str,
    session_id: Option<&str>,
) -> Result<Response> {
    crate::validation::validate_id(stage_id).context("invalid block stage id")?;
    let failed = disputable_failures(work_dir, stage_id);
    if !failed.is_empty() {
        return Ok(Response::Error {
            message: failed_completion_refusal(stage_id, &failed),
        });
    }
    block_for_session(work_dir, stage_id, reason, session_id)
}

/// The failures of the latest `ToolFailed` completion attempt that a dispute
/// answers.
#[derive(Debug, Default)]
struct DisputableFailures {
    /// Indices of the failing `acceptance-<i>` criteria.
    criteria: Vec<usize>,
    /// Indices of the failing `artifact-<i>` patterns.
    artifacts: Vec<usize>,
    /// Indices of the failing `wiring-<i>` checks.
    wiring: Vec<usize>,
    /// Indices of the failing `wiring_tests-<i>` tests.
    wiring_tests: Vec<usize>,
    /// The stage's frozen contracts when the contract check failed. The
    /// attempt records one outcome for the whole check, so it cannot say
    /// which contract failed; the refusal offers every contract.
    contracts: Vec<String>,
    /// Whether the impact-selected tests failed.
    impact_tests: bool,
}

impl DisputableFailures {
    fn is_empty(&self) -> bool {
        self.criteria.is_empty()
            && self.artifacts.is_empty()
            && self.wiring.is_empty()
            && self.wiring_tests.is_empty()
            && self.contracts.is_empty()
            && !self.impact_tests
    }
}

/// What failed in the current session's latest signed `ToolFailed` completion
/// attempt. A record that cannot be read yields nothing: the agent's way to
/// stop must not hinge on it.
fn disputable_failures(work_dir: &Path, stage_id: &str) -> DisputableFailures {
    let stage = match load_stage(stage_id, work_dir) {
        Ok(stage) => stage,
        Err(error) => {
            tracing::warn!(stage_id, error = %format!("{error:#}"), "block: stage unreadable");
            return DisputableFailures::default();
        }
    };
    let Some(session) = stage.session.as_deref() else {
        return DisputableFailures::default();
    };
    match load_trusted_session_checkpoint(stage_id, session, work_dir) {
        Ok(checkpoint) => checkpoint
            .and_then(|checkpoint| checkpoint.latest)
            .filter(|latest| latest.phase == CompletionPhase::ToolFailed)
            .map(|latest| failures_in(&stage, &latest.verification.criteria))
            .unwrap_or_default(),
        Err(error) => {
            tracing::warn!(
                stage_id,
                session_id = %session,
                error = %format!("{error:#}"),
                "block: completion record unreadable"
            );
            DisputableFailures::default()
        }
    }
}

fn failures_in(stage: &Stage, results: &[CriterionResult]) -> DisputableFailures {
    let failed = || results.iter().filter(|result| !result.passed);
    let named = |label: &str| failed().any(|result| result.id == label);
    let contracts = if named(CONTRACTS_LABEL) {
        stage
            .contracts
            .iter()
            .map(|contract| contract.id.clone())
            .collect()
    } else {
        Vec::new()
    };
    DisputableFailures {
        criteria: control_block_gaps::failed_indices(results, "acceptance"),
        artifacts: control_block_gaps::failed_indices(results, "artifact"),
        wiring: control_block_gaps::failed_indices(results, "wiring"),
        wiring_tests: control_block_gaps::failed_indices(results, "wiring_tests"),
        contracts,
        impact_tests: named(IMPACT_TESTS_LABEL),
    }
}

/// The refusal: one route for each kind of failure the attempt recorded, so a
/// stage that failed only impact-selected tests is not pointed at criteria.
fn failed_completion_refusal(stage_id: &str, failed: &DisputableFailures) -> String {
    let mut routes = Vec::new();
    if !failed.criteria.is_empty() {
        routes.push(control_block_gaps::criteria_route(
            stage_id,
            &failed.criteria,
        ));
    }
    routes.extend(control_block_gaps::gap_routes(stage_id, failed));
    if failed.impact_tests {
        routes.push(format!(
            "Failing impact-selected tests: `loom stage dispute-criteria {stage_id} --field \
             impact-tests --test <id> --reason \"...\"` names each failing selection."
        ));
    }
    if !failed.contracts.is_empty() {
        routes.push(contracts_route(stage_id, &failed.contracts));
    }
    format!(
        "cannot block stage '{stage_id}': the latest `loom stage complete` shows failing \
         checks. A failing check is disputed, never blocked. {} Other routes: `loom stage \
         dispute-findings` for open review findings, `loom stage dispute-integrity` for \
         test-integrity events, `loom stage dispute-criteria --field wiring` or `--field \
         wiring-tests` for wiring entries, `--field artifacts` for artifact entries. A filing \
         past the dispute budget goes to the operator automatically.",
        routes.join(" ")
    )
}

/// The contract check fails on a contract's own test and on frozen-file drift,
/// and records one outcome for both: `loom check --contracts` tells them
/// apart, and drift has its own repair.
fn contracts_route(stage_id: &str, contracts: &[String]) -> String {
    let flags: Vec<String> = contracts
        .iter()
        .map(|id| format!("--contract {id}"))
        .collect();
    format!(
        "The frozen contract check failed and does not say which contract or why: `loom check \
         {stage_id} --contracts` shows which fail. A frozen file that drifted is repaired with \
         `loom stage contracts restore {stage_id}`, which needs no dispute. A contract that is \
         wrong is disputed with `loom stage dispute-contract {stage_id} {} --reason \"...\"`, \
         which lists every contract of the stage; drop the flags of those that pass.",
        flags.join(" ")
    )
}

/// Apply `Blocked` to `stage_id`, recording `reason` as its close reason.
///
/// A refused transition comes back as [`Response::Error`] rather than an
/// `Err`: it is an answer about the stage, not a failure of the request, and
/// the message names the status it was refused from because an agent reading
/// the reply has no other way to learn why. "Already blocked" and "already
/// completed" call for different next moves.
pub(crate) fn handle_block_stage(
    work_dir: &Path,
    stage_id: &str,
    reason: &str,
) -> Result<Response> {
    block_for_session(work_dir, stage_id, reason, None)
}

fn block_for_session(
    work_dir: &Path,
    stage_id: &str,
    reason: &str,
    session_id: Option<&str>,
) -> Result<Response> {
    // The id arrives unvalidated from the wire and is resolved to a path
    // below; traversal shapes die before any file is touched.
    crate::validation::validate_id(stage_id).context("invalid block stage id")?;

    let mut refusal = None;
    update_stage(stage_id, work_dir, |stage| {
        if session_id.is_some_and(|id| !super::ownership::matches_attempt(stage, id)) {
            refusal = Some("request belongs to a stale stage session".to_string());
            return Ok(());
        }
        if let Err(error) = apply_block(stage, reason) {
            refusal = Some(format!("cannot block stage '{stage_id}': {error:#}"));
        }
        Ok(())
    })
    .with_context(|| format!("failed to persist blocked state for stage '{stage_id}'"))?;

    Ok(match refusal {
        Some(message) => Response::Error { message },
        None => Response::Ok,
    })
}

/// Move `stage` to `Blocked` with `reason` as its close reason; a refused
/// transition changes nothing. The daemon's handler and the CLI's
/// no-daemon fallback both block this way.
///
/// A block closes the attempt with its own reason, so the failure and the
/// review reason the left status recorded go into `issue_history`: an
/// earlier attempt's crash evidence would re-arm auto-retry and hide this
/// block from attention and from the daemon's retirement of the session,
/// and a review reason would describe a wait the block ended.
// Kept beside `Orchestrator::block_stage` (orchestrator/core/stage_block.rs): this mutates
// a `Stage` the caller persists, with no orchestrator or graph, and retires issue history.
pub(crate) fn apply_block(stage: &mut Stage, reason: &str) -> Result<()> {
    stage.try_mark_blocked()?;
    stage.close_reason = Some(reason.to_string());
    stage.retire_failure(
        "blocked by loom stage block",
        crate::models::stage::IssueDisposition::Superseded,
    );
    stage.retire_review_reason(
        "blocked by loom stage block",
        crate::models::stage::IssueDisposition::Superseded,
    );
    stage.updated_at = Utc::now();
    Ok(())
}

#[path = "control_block_gaps.rs"]
mod control_block_gaps;

#[cfg(test)]
#[path = "control_block_agent_tests.rs"]
mod agent_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::{Stage, StageStatus};
    use crate::verify::transitions::{load_stage, save_stage};
    use tempfile::TempDir;

    fn stage_in(work_dir: &Path, stage_id: &str, status: StageStatus) {
        let mut stage = Stage::new(stage_id.to_string(), None);
        stage.id = stage_id.to_string();
        stage.status = status;
        save_stage(&stage, work_dir).unwrap();
    }

    #[test]
    fn blocking_an_executing_stage_records_the_reason() {
        let temp = TempDir::new().unwrap();
        stage_in(temp.path(), "build-api", StageStatus::Executing);

        let response =
            handle_block_stage(temp.path(), "build-api", "criterion 41 is unrunnable").unwrap();

        assert!(matches!(response, Response::Ok));
        let stage = load_stage("build-api", temp.path()).unwrap();
        assert_eq!(stage.status, StageStatus::Blocked);
        assert_eq!(
            stage.close_reason.as_deref(),
            Some("criterion 41 is unrunnable")
        );
    }

    #[test]
    fn a_block_clears_a_prior_attempts_failure_info() {
        use crate::models::failure::{FailureInfo, FailureType};

        // A stage can reach `Executing` with a `failure_info` still set
        // (a provision block records `InfrastructureError` before the
        // operator retries it); blocking archives it. Entering `Queued`
        // already archives the failure, including on a retry without
        // `--force`, so this seeds the failure on the stage directly.
        for prior in [FailureType::SessionCrash, FailureType::InfrastructureError] {
            let temp = TempDir::new().unwrap();
            let mut stage = Stage::new("build-api".to_string(), None);
            stage.id = "build-api".to_string();
            stage.status = StageStatus::Executing;
            stage.failure_info = Some(FailureInfo {
                failure_type: prior.clone(),
                detected_at: Utc::now(),
                evidence: vec!["an earlier attempt".to_string()],
            });
            save_stage(&stage, temp.path()).unwrap();

            let response =
                handle_block_stage(temp.path(), "build-api", "spec is ambiguous").unwrap();

            assert!(matches!(response, Response::Ok), "prior {prior:?}");
            let stage = load_stage("build-api", temp.path()).unwrap();
            assert_eq!(stage.status, StageStatus::Blocked, "prior {prior:?}");
            assert_eq!(stage.close_reason.as_deref(), Some("spec is ambiguous"));
            assert!(
                stage.failure_info.is_none(),
                "prior {prior:?} must be cleared"
            );
            let archived: Vec<_> = stage
                .issue_history
                .iter()
                .map(|issue| (issue.failure_type.clone(), issue.ended_by.as_str()))
                .collect();
            assert_eq!(
                archived,
                [(Some(prior.clone()), "blocked by loom stage block")],
                "prior {prior:?} must be archived"
            );
        }
    }

    #[test]
    fn blocking_a_stage_awaiting_review_archives_its_review_reason() {
        let temp = TempDir::new().unwrap();
        let mut stage = Stage::new("build-api".to_string(), None);
        stage.id = "build-api".to_string();
        stage.status = StageStatus::NeedsHumanReview;
        stage.review_reason = Some("zero commits".to_string());
        save_stage(&stage, temp.path()).unwrap();

        let response = handle_block_stage(temp.path(), "build-api", "redo by hand").unwrap();

        assert!(matches!(response, Response::Ok));
        let stage = load_stage("build-api", temp.path()).unwrap();
        assert_eq!(stage.status, StageStatus::Blocked);
        assert_eq!(stage.review_reason, None);
        assert_eq!(stage.issue_history.len(), 1);
        assert_eq!(stage.issue_history[0].evidence, ["zero commits"]);
        assert_eq!(
            stage.issue_history[0].ended_by,
            "blocked by loom stage block"
        );
    }

    #[test]
    fn a_refused_transition_names_the_status_it_was_refused_from() {
        let temp = TempDir::new().unwrap();
        stage_in(temp.path(), "build-api", StageStatus::Completed);

        let response = handle_block_stage(temp.path(), "build-api", "too late").unwrap();

        match response {
            Response::Error { message } => assert!(
                message.contains("build-api") && message.contains("Completed"),
                "message must name the stage and the status it is in: {message}"
            ),
            other => panic!("expected a refusal, got {other:?}"),
        }
        // The refusal must not have half-applied: no reason, no status change.
        let stage = load_stage("build-api", temp.path()).unwrap();
        assert_eq!(stage.status, StageStatus::Completed);
        assert!(stage.close_reason.is_none());
    }

    #[test]
    fn a_stage_id_shaped_like_a_path_is_refused() {
        let temp = TempDir::new().unwrap();

        assert!(handle_block_stage(temp.path(), "../../tmp/escape", "x").is_err());
    }
}
