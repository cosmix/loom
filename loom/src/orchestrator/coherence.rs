//! Pure judgments about whether an `Executing` stage names a session record
//! that could plausibly be the agent actually doing the work.
//!
//! An adjudication (or, for a knowledge stage, any non-`Knowledge`) session
//! carries the stage's own `stage_id`, so `stage.session` alone cannot tell
//! it apart from the stage's real worker. This module is the single place
//! that answers "does this pointer actually describe a working agent?", and
//! the escalation that follows when it does not — used both by the per-tick
//! watchdog (`core/coherence.rs`) and by `loom status`/`loom repair`.

use std::path::Path;

use anyhow::Result;
use chrono::Utc;

use crate::models::failure::{FailureInfo, FailureType};
use crate::models::session::{Session, SessionType};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::session_registry::live_sessions_for_stage_of_type;

/// The session kind that works a stage of this type.
pub fn worker_session_type(stage: &Stage) -> SessionType {
    match stage.stage_type {
        StageType::Knowledge => SessionType::Knowledge,
        _ => SessionType::Stage,
    }
}

/// Whether `stage` runs a contract phase, whose `Contract` session works the
/// stage before its `Stage` session does: v2 standard stages only.
fn has_contract_phase(stage: &Stage) -> bool {
    stage.plan_version == 2 && stage.stage_type == StageType::Standard
}

/// Whether a session of `kind` is one that works `stage`: its worker kind,
/// or its contract writer when it has a contract phase.
pub fn is_worker_session_type(stage: &Stage, kind: SessionType) -> bool {
    kind == worker_session_type(stage)
        || (kind == SessionType::Contract && has_contract_phase(stage))
}

/// Live sessions of `stage`'s worker kind. A stage with a contract phase and
/// no live worker session is in that phase, so its live contract writers are
/// returned instead.
pub fn live_worker_sessions(work_dir: &Path, stage: &Stage) -> Result<Vec<Session>> {
    let live = live_sessions_for_stage_of_type(work_dir, &stage.id, worker_session_type(stage))?;
    if !live.is_empty() || !has_contract_phase(stage) {
        return Ok(live);
    }
    live_sessions_for_stage_of_type(work_dir, &stage.id, SessionType::Contract)
}

/// Why an `Executing` stage does not describe a working agent, if it does
/// not. `assigned` is the record named by `stage.session`, if one was found.
///
/// Judges identity only — never session status or PID liveness. Crash
/// detection owns dead processes, and a `Completed` record can legitimately
/// precede the stage's own completion by a tick.
pub fn executing_stage_incoherence(stage: &Stage, assigned: Option<&Session>) -> Option<String> {
    if stage.status != StageStatus::Executing {
        return None;
    }

    let Some(session_id) = stage.session.as_deref() else {
        return Some("Executing with no session assigned".to_string());
    };

    let Some(session) = assigned else {
        return Some(format!(
            "Executing but its session '{session_id}' has no record on disk"
        ));
    };

    if session.stage_id.as_deref() != Some(stage.id.as_str()) {
        return Some(format!(
            "session '{session_id}' belongs to stage {}, not this one",
            session.stage_id.as_deref().unwrap_or("<none>")
        ));
    }

    let kind = session.session_type;
    if !is_worker_session_type(stage, kind) {
        let expected = worker_session_type(stage);
        return Some(format!(
            "session '{session_id}' is of kind {kind}, not the stage's worker kind {expected}"
        ));
    }

    None
}

/// The record `stage.session` names, if any.
pub fn load_assigned_session(work_dir: &Path, stage: &Stage) -> Result<Option<Session>> {
    let Some(session_id) = stage.session.as_deref() else {
        return Ok(None);
    };
    crate::fs::session_files::load_session_exact(work_dir, session_id)
}

/// Escalate an incoherent Executing stage: Blocked with an infrastructure
/// failure naming the reason, session pointer cleared. Returns `None` if the
/// stage was no longer Executing under the lock.
pub fn block_incoherent_stage(
    work_dir: &Path,
    stage_id: &str,
    reason: &str,
) -> Result<Option<Stage>> {
    let mut still_executing = false;
    let stage = crate::verify::transitions::update_stage(stage_id, work_dir, |s| {
        if s.status != StageStatus::Executing {
            return Ok(());
        }
        still_executing = true;
        s.try_mark_blocked()?;
        s.failure_info = Some(FailureInfo {
            failure_type: FailureType::InfrastructureError,
            detected_at: Utc::now(),
            evidence: vec![
                reason.to_string(),
                "no live worker session was found; the stage was Executing with nobody working"
                    .to_string(),
            ],
        });
        s.release_session();
        Ok(())
    })?;
    Ok(still_executing.then_some(stage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::session::SessionStatus;

    fn stage_with(status: StageStatus, session: Option<&str>, stage_type: StageType) -> Stage {
        let mut stage = Stage::new("t".to_string(), None);
        stage.id = "alpha".to_string();
        stage.status = status;
        stage.session = session.map(str::to_string);
        stage.stage_type = stage_type;
        stage
    }

    fn session_of(stage_id: &str, session_type: SessionType) -> Session {
        let mut session = Session::new();
        session.stage_id = Some(stage_id.to_string());
        session.session_type = session_type;
        session.status = SessionStatus::Running;
        session
    }

    #[test]
    fn executing_stage_naming_an_adjudicator_is_incoherent() {
        let session = session_of("alpha", SessionType::Adjudication);
        let stage = stage_with(
            StageStatus::Executing,
            Some(session.id.as_str()),
            StageType::Standard,
        );
        assert!(executing_stage_incoherence(&stage, Some(&session)).is_some());
    }

    #[test]
    fn executing_stage_naming_its_own_stage_session_is_coherent() {
        let session = session_of("alpha", SessionType::Stage);
        let stage = stage_with(
            StageStatus::Executing,
            Some(session.id.as_str()),
            StageType::Standard,
        );
        assert!(executing_stage_incoherence(&stage, Some(&session)).is_none());
    }

    #[test]
    fn executing_knowledge_stage_naming_a_knowledge_session_is_coherent() {
        let session = session_of("alpha", SessionType::Knowledge);
        let stage = stage_with(
            StageStatus::Executing,
            Some(session.id.as_str()),
            StageType::Knowledge,
        );
        assert!(executing_stage_incoherence(&stage, Some(&session)).is_none());
    }

    #[test]
    fn a_non_executing_stage_is_never_incoherent() {
        let stage = stage_with(StageStatus::Queued, None, StageType::Standard);
        assert!(executing_stage_incoherence(&stage, None).is_none());
    }

    #[test]
    fn worker_session_type_is_knowledge_only_for_knowledge_stages() {
        let knowledge = stage_with(StageStatus::Executing, None, StageType::Knowledge);
        let standard = stage_with(StageStatus::Executing, None, StageType::Standard);
        assert_eq!(worker_session_type(&knowledge), SessionType::Knowledge);
        assert_eq!(worker_session_type(&standard), SessionType::Stage);
    }

    /// The contract writer is a v2 standard stage's agent for its first
    /// phase; the per-tick watchdog must not block the stage for running it.
    #[test]
    fn executing_v2_standard_stage_naming_its_contract_session_is_coherent() {
        let session = session_of("alpha", SessionType::Contract);
        let mut stage = stage_with(
            StageStatus::Executing,
            Some(session.id.as_str()),
            StageType::Standard,
        );
        stage.plan_version = 2;
        assert!(executing_stage_incoherence(&stage, Some(&session)).is_none());
    }

    /// v1 stages and non-standard stages have no contract phase, so a
    /// contract session there is as foreign as any other kind.
    #[test]
    fn a_contract_session_is_incoherent_without_a_contract_phase() {
        let session = session_of("alpha", SessionType::Contract);
        for (plan_version, stage_type) in [
            (1, StageType::Standard),
            (2, StageType::Knowledge),
            (2, StageType::IntegrationVerify),
        ] {
            let mut stage = stage_with(
                StageStatus::Executing,
                Some(session.id.as_str()),
                stage_type,
            );
            stage.plan_version = plan_version;
            let reason = executing_stage_incoherence(&stage, Some(&session));
            assert!(reason.is_some(), "v{plan_version} {stage_type:?}");
        }
    }
}
