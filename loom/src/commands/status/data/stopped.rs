//! What the status payload carries only for a stage that has stopped: the
//! reason it was blocked, the acceptance criteria an operator can amend, and
//! the terminal history of its last tmux session.

use crate::fs::work_dir::WorkDir;
use crate::models::session::{Session, SessionBackendKind, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::terminal::tmux::has_history;

use super::resolver::is_live;

/// Most acceptance criteria one stage contributes to a payload the whole
/// dashboard shares.
const MAX_CRITERIA: usize = 100;

/// Why a blocked stage was blocked. It is what the guidance for a block quotes
/// (`render::blocked_guidance`); no renderer reads the close reason of any other
/// status, and it runs to `MAX_CLOSE_REASON_CHARS` characters in a payload sent
/// every second, so every other stage carries none.
pub(super) fn blocked_close_reason(stage: &Stage) -> Option<String> {
    if stage.status != StageStatus::Blocked {
        return None;
    }
    stage.close_reason.clone()
}

/// The acceptance criteria of a blocked stage, in the order `loom stage amend
/// --field acceptance --index` counts them from 0. Every other stage carries
/// none: a criterion's position matters to an operator only while the stage is
/// blocked, and the commands run to hundreds of characters.
pub(super) fn blocked_acceptance(stage: &Stage) -> Vec<String> {
    if stage.status != StageStatus::Blocked {
        return Vec::new();
    }
    stage
        .acceptance
        .iter()
        .take(MAX_CRITERIA)
        .map(|criterion| criterion.command().to_owned())
        .collect()
}

/// The newest tmux session of `stage` that has ended and left a terminal
/// history file. Only the newest ended session is asked: the history of an
/// earlier attempt is not what the stage last showed.
///
/// An adjudication judge is not part of the stage's own terminal, so its
/// sessions are skipped, as they are by `loom attach`.
pub(super) fn terminal_history_session(
    stage: &Stage,
    sessions: &[Session],
    work_dir: &WorkDir,
) -> Option<String> {
    let latest = sessions
        .iter()
        .filter(|session| {
            session.stage_id.as_deref() == Some(stage.id.as_str())
                && session.backend == SessionBackendKind::Tmux
                && session.session_type != SessionType::Adjudication
                && !is_live(session)
        })
        .max_by_key(|session| session.created_at)?;
    has_history(work_dir.root(), &latest.id).then(|| latest.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::session::SessionStatus;
    use crate::models::stage::AcceptanceCriterion;
    use crate::orchestrator::terminal::tmux::write_history;
    use chrono::{Duration, Utc};
    use tempfile::TempDir;

    fn work_dir() -> (TempDir, WorkDir) {
        let temp = TempDir::new().unwrap();
        let work = WorkDir::new(temp.path()).unwrap();
        work.initialize().unwrap();
        (temp, work)
    }

    fn ended_session(stage_id: &str, age_secs: i64) -> Session {
        let mut session = Session::new();
        session.assign_to_stage(stage_id.to_string());
        session.backend = SessionBackendKind::Tmux;
        session.status = SessionStatus::Completed;
        session.created_at = Utc::now() - Duration::seconds(age_secs);
        session
    }

    fn stage(status: StageStatus) -> Stage {
        let mut stage = Stage::new("server".to_string(), None);
        stage.id = "server".to_string();
        stage.status = status;
        stage.acceptance = vec![
            AcceptanceCriterion::Simple("cargo build".to_string()),
            AcceptanceCriterion::Simple("cargo test".to_string()),
        ];
        stage
    }

    #[test]
    fn only_a_blocked_stage_carries_its_close_reason() {
        let mut blocked = stage(StageStatus::Blocked);
        blocked.close_reason = Some("needs a decision".to_string());
        assert_eq!(
            blocked_close_reason(&blocked).as_deref(),
            Some("needs a decision")
        );
        for status in [
            StageStatus::Skipped,
            StageStatus::Completed,
            StageStatus::CompletedWithFailures,
            StageStatus::NeedsHumanReview,
        ] {
            blocked.status = status;
            assert_eq!(blocked_close_reason(&blocked), None);
        }
    }

    #[test]
    fn a_blocked_stage_lists_its_criteria_in_amend_order() {
        assert_eq!(
            blocked_acceptance(&stage(StageStatus::Blocked)),
            ["cargo build", "cargo test"]
        );
    }

    #[test]
    fn a_stage_that_is_not_blocked_lists_no_criteria() {
        assert!(blocked_acceptance(&stage(StageStatus::Executing)).is_empty());
    }

    #[test]
    fn the_criteria_list_is_bounded() {
        let mut long = stage(StageStatus::Blocked);
        long.acceptance = (0..MAX_CRITERIA + 20)
            .map(|index| AcceptanceCriterion::Simple(format!("check {index}")))
            .collect();

        assert_eq!(blocked_acceptance(&long).len(), MAX_CRITERIA);
    }

    #[test]
    fn the_newest_ended_session_with_a_history_file_is_named() {
        let (_temp, work) = work_dir();
        let older = ended_session("server", 600);
        let newer = ended_session("server", 60);
        write_history(work.root(), &older.id, b"old").unwrap();
        write_history(work.root(), &newer.id, b"new").unwrap();

        let named =
            terminal_history_session(&stage(StageStatus::Blocked), &[older, newer.clone()], &work);

        assert_eq!(named, Some(newer.id));
    }

    #[test]
    fn a_newest_session_without_a_file_names_nothing() {
        let (_temp, work) = work_dir();
        let older = ended_session("server", 600);
        let newer = ended_session("server", 60);
        write_history(work.root(), &older.id, b"old").unwrap();

        assert_eq!(
            terminal_history_session(&stage(StageStatus::Blocked), &[older, newer], &work),
            None
        );
    }

    #[test]
    fn other_stages_native_sessions_and_judges_are_not_named() {
        let (_temp, work) = work_dir();
        let elsewhere = ended_session("client", 60);
        let mut native = ended_session("server", 60);
        native.backend = SessionBackendKind::Native;
        let mut judge = ended_session("server", 60);
        judge.session_type = SessionType::Adjudication;
        for session in [&elsewhere, &native, &judge] {
            write_history(work.root(), &session.id, b"screen").unwrap();
        }

        assert_eq!(
            terminal_history_session(
                &stage(StageStatus::Blocked),
                &[elsewhere, native, judge],
                &work
            ),
            None
        );
    }
}
