//! A relayed `check`: the session asks the daemon to run the stage's gate on
//! the host. The handler validates who asked and what they saw, queues (or
//! joins) a `Check` host job, and settles at once with the job id, because a
//! request left `Applying` would be settled `UnknownAfterRestart` on the next
//! pass. The session follows the job through the state directory
//! (`host_verify::wait`).

use chrono::Utc;

use crate::git::worktree::WorktreeGit;
use crate::host_verify::submit::{is_lower_hex, submit, Submission, SubmitRequest};
use crate::host_verify::JobKind;
use crate::models::session::{Session, SessionType};
use crate::models::stage::{Stage, StageType};
use crate::relay::HostJobRequest;
use crate::verify::transitions::load_stage;

use super::apply::{is_live_judge, require_owner};
use super::commit::pinned_worktree;
use super::{InboxHost, Settle};

/// Validate and queue one relayed `check`.
pub(super) fn apply_check(
    host: &dyn InboxHost,
    stage_id: &str,
    record: &Session,
    request: &HostJobRequest,
    request_id: &str,
) -> Settle {
    let stage = match admit_session(host, stage_id, record) {
        Ok(stage) => stage,
        Err(reason) => return Settle::Refused(reason),
    };
    if let Err(reason) = validate(host, &stage, request) {
        return Settle::Refused(reason);
    }
    settle_submission(submit(
        host.work_dir(),
        &SubmitRequest {
            stage_id,
            session_id: &record.id,
            request_id,
            kind: JobKind::Check,
            head: &request.head,
            tree_digest: &request.tree_digest,
            no_cache: request.no_cache,
            criteria: request.criteria.clone(),
        },
        Utc::now(),
    ))
}

/// Maps a submitted host job to the request's settlement. Shared with the
/// `complete` relay, which settles its submissions the same way.
pub(super) fn settle_submission(submission: anyhow::Result<Submission>) -> Settle {
    match submission {
        Ok(Submission::Queued(job)) => Settle::Applied(Some(format!("host job {} queued", job.id))),
        Ok(Submission::Joined(job)) => Settle::Applied(Some(format!("host job {} joined", job.id))),
        Ok(Submission::Superseded { cancelled, queued }) => Settle::Applied(Some(format!(
            "host job {} queued; {} cancelled",
            queued.id, cancelled.id
        ))),
        Ok(Submission::Refused(reason)) => Settle::Refused(reason),
        Err(error) => Settle::Refused(format!("could not be applied: {error:#}")),
    }
}

/// A stage or knowledge session must own the stage; a judge must be the
/// live adjudication session of this stage.
fn admit_session(host: &dyn InboxHost, stage_id: &str, record: &Session) -> Result<Stage, String> {
    if record.session_type != SessionType::Adjudication {
        return require_owner(host.work_dir(), stage_id, record);
    }
    if !is_live_judge(host, record) {
        return Err(format!(
            "only the live adjudication session for stage '{stage_id}' may request its check"
        ));
    }
    load_stage(stage_id, host.work_dir())
        .map_err(|error| format!("stage '{stage_id}' could not be loaded: {error:#}"))
}

/// The request's head and tree digest are well-formed hex ids. Shared with the
/// `complete` relay.
pub(super) fn validate_ids(request: &HostJobRequest) -> Result<(), String> {
    if !is_lower_hex(&request.head, &[40, 64]) {
        return Err("head must be 40 or 64 lowercase hex characters".to_string());
    }
    if !is_lower_hex(&request.tree_digest, &[64]) {
        return Err("tree_digest must be 64 lowercase hex characters".to_string());
    }
    Ok(())
}

fn validate(host: &dyn InboxHost, stage: &Stage, request: &HostJobRequest) -> Result<(), String> {
    validate_ids(request)?;
    let head = checkout_head(host, stage)?;
    if head != request.head {
        return Err(format!(
            "the checkout's HEAD is {head}, not {}: run loom check again",
            request.head
        ));
    }
    Ok(())
}

/// HEAD of the stage's checkout, read with git pinned to that directory: the
/// stage worktree, or the repository root for a knowledge stage.
fn checkout_head(host: &dyn InboxHost, stage: &Stage) -> Result<String, String> {
    let git = if stage.stage_type == StageType::Knowledge {
        WorktreeGit::discovered(host.repo_root())
    } else {
        pinned_worktree(host.repo_root(), stage)?
    };
    let output = git
        .run(&["rev-parse", "HEAD"])
        .map_err(|error| format!("the checkout's HEAD could not be read: {error:#}"))?;
    if !output.status.success() {
        return Err(format!(
            "the checkout's HEAD could not be read: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Command;

    use chrono::Utc;
    use serde_json::{json, Value};

    use crate::fs::inbox::write_entry;
    use crate::host_verify::store::load_current;
    use crate::host_verify::JobState;
    use crate::models::session::{Session, SessionStatus, SessionType};
    use crate::models::stage::StageStatus;
    use crate::relay::{AgentRole, RequestKind};

    use super::super::test_support::{entry_for, fixture, Fixture, STAGE};
    use super::super::{run_pass, Settle};

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(["-c", "commit.gpgsign=false", "-c", "user.name=t"])
            .args(["-c", "user.email=t@t"])
            .args(args)
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {output:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// A repository at the fixture's repo root with `s1`'s registered
    /// worktree; returns the worktree's HEAD.
    fn repository(fx: &Fixture) -> String {
        git(&fx.repo_root, &["init", "-q", "-b", "main"]);
        git(
            &fx.repo_root,
            &["commit", "-q", "--allow-empty", "-m", "base"],
        );
        let add = ["worktree", "add", "-q", ".worktrees/s1", "-b", "loom/s1"];
        git(&fx.repo_root, &add);
        let worktree = fx.repo_root.join(".worktrees").join(STAGE);
        git(&worktree, &["rev-parse", "HEAD"])
    }

    fn request(head: &str) -> Value {
        json!({"head": head, "tree_digest": "0".repeat(64), "no_cache": false})
    }

    /// Relay `payload` as `agent` and return how the drain settled it.
    fn settle(
        fx: &Fixture,
        record: &Session,
        agent: AgentRole,
        payload: Value,
        alive: bool,
    ) -> Settle {
        let mut entry = entry_for(record, RequestKind::Check, payload);
        entry.agent = agent;
        write_entry(&fx.work_dir, &entry).unwrap();
        let mut host = fx.host(alive);
        let report = run_pass(&mut host, &fx.tick(Utc::now()));
        let found = report.settled.into_iter().find(|(id, _)| *id == entry.id);
        found.unwrap().1
    }

    fn refused(settled: Settle) -> String {
        match settled {
            Settle::Refused(reason) => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    fn queued_check(fx: &Fixture) -> bool {
        load_current(&fx.work_dir, STAGE)
            .unwrap()
            .is_some_and(|job| job.state == JobState::Queued)
    }

    #[test]
    fn an_owning_stage_session_queues_one_check_job() {
        let fx = fixture();
        let head = repository(&fx);
        let record = fx.record(SessionType::Stage, SessionStatus::Running);
        fx.stage(StageStatus::Executing, Some(&record.id));
        let settled = settle(&fx, &record, AgentRole::Main, request(&head), true);
        assert!(matches!(&settled, Settle::Applied(Some(note)) if note.starts_with("host job ")));
        assert!(queued_check(&fx));
    }

    #[test]
    fn a_subagent_of_the_owning_session_is_queued_too() {
        let fx = fixture();
        let head = repository(&fx);
        let record = fx.record(SessionType::Stage, SessionStatus::Running);
        fx.stage(StageStatus::Executing, Some(&record.id));
        let settled = settle(&fx, &record, AgentRole::Subagent, request(&head), true);
        assert!(matches!(settled, Settle::Applied(Some(_))), "{settled:?}");
        assert!(queued_check(&fx));
    }

    #[test]
    fn a_live_judge_of_the_stage_is_queued() {
        let fx = fixture();
        let head = repository(&fx);
        let judge = fx.record(SessionType::Adjudication, SessionStatus::Running);
        fx.stage(StageStatus::NeedsAdjudication, None);
        let settled = settle(&fx, &judge, AgentRole::Main, request(&head), true);
        assert!(matches!(settled, Settle::Applied(Some(_))), "{settled:?}");
        assert!(queued_check(&fx));
    }

    #[test]
    fn a_session_that_does_not_own_the_stage_is_refused() {
        let fx = fixture();
        let head = repository(&fx);
        let record = fx.record(SessionType::Stage, SessionStatus::Running);
        fx.stage(StageStatus::Executing, Some("someone-else"));
        let reason = refused(settle(&fx, &record, AgentRole::Main, request(&head), true));
        assert!(reason.contains("not the active session"), "{reason}");
        assert!(!queued_check(&fx));
    }

    #[test]
    fn a_judge_that_is_not_alive_is_refused() {
        let fx = fixture();
        let head = repository(&fx);
        let judge = fx.record(SessionType::Adjudication, SessionStatus::Running);
        fx.stage(StageStatus::NeedsAdjudication, None);
        let reason = refused(settle(&fx, &judge, AgentRole::Main, request(&head), false));
        assert!(reason.contains("live adjudication session"), "{reason}");
        assert!(!queued_check(&fx));
    }

    #[test]
    fn a_judge_of_another_stage_is_refused() {
        let fx = fixture();
        let head = repository(&fx);
        let mut judge = Session::new();
        judge.session_type = SessionType::Adjudication;
        judge.assign_to_stage("other".to_string());
        judge.status = SessionStatus::Running;
        crate::fs::session_files::save_session(&judge, &fx.work_dir).unwrap();
        fx.stage(StageStatus::NeedsAdjudication, None);
        let settled = settle(&fx, &judge, AgentRole::Main, request(&head), true);
        assert!(matches!(settled, Settle::Refused(_)), "{settled:?}");
        assert!(!queued_check(&fx));
    }

    #[test]
    fn a_malformed_head_or_digest_is_refused() {
        let fx = fixture();
        repository(&fx);
        let record = fx.record(SessionType::Stage, SessionStatus::Running);
        fx.stage(StageStatus::Executing, Some(&record.id));
        let bad_head = refused(settle(&fx, &record, AgentRole::Main, request("ABC"), true));
        assert!(bad_head.contains("head must be"), "{bad_head}");
        let head = "a".repeat(40);
        let payload = json!({"head": head, "tree_digest": "xyz", "no_cache": false});
        let bad_digest = refused(settle(&fx, &record, AgentRole::Main, payload, true));
        assert!(bad_digest.contains("tree_digest must be"), "{bad_digest}");
    }

    #[test]
    fn a_head_that_is_not_the_checkouts_is_refused() {
        let fx = fixture();
        repository(&fx);
        let record = fx.record(SessionType::Stage, SessionStatus::Running);
        fx.stage(StageStatus::Executing, Some(&record.id));
        let wrong = "a".repeat(40);
        let reason = refused(settle(&fx, &record, AgentRole::Main, request(&wrong), true));
        assert!(reason.contains("run loom check again"), "{reason}");
        assert!(!queued_check(&fx));
    }

    #[test]
    fn a_submission_that_fails_settles_refused() {
        let fx = fixture();
        let head = repository(&fx);
        let record = fx.record(SessionType::Stage, SessionStatus::Running);
        fx.stage(StageStatus::Executing, Some(&record.id));
        let elsewhere = fx.work_dir.join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, fx.work_dir.join("verification")).unwrap();
        let reason = refused(settle(&fx, &record, AgentRole::Main, request(&head), true));
        assert!(reason.contains("could not be applied"), "{reason}");
    }
}
