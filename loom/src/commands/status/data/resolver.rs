//! The merge resolver as the status collector sees it: which session speaks
//! for a stage in a merge state, and the resolver facts the summary carries.
//!
//! Status collection never writes. `stage.session` is never pointed at a
//! resolver (everything else reads it as the stage's worker), so the resolver
//! is found here, on read, from the session records.

use crate::fs::work_dir::WorkDir;
use crate::models::session::{Session, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::merge_resolver_attempts;

use super::sanitize::valid_stage_id;

/// Whether the daemon resolves a merge for a stage in `status`.
fn in_merge_state(status: &StageStatus) -> bool {
    matches!(
        status,
        StageStatus::MergeConflict | StageStatus::MergeBlocked
    )
}

/// A session that is not terminal and whose recorded pid, if any, is alive.
pub(super) fn is_live(session: &Session) -> bool {
    !session.status.is_terminal() && session.pid.is_none_or(crate::process::is_process_alive)
}

/// The live merge resolver for a stage: the newest live `Merge` session naming
/// the stage, as the web terminal picks it (`web::terminal::resolve`).
/// Deliberately not `find_live_merge_session_for_stage`, which deletes stale
/// signal files.
fn live_merge_resolver<'a>(stage: &Stage, sessions: &'a [Session]) -> Option<&'a Session> {
    sessions
        .iter()
        .filter(|s| {
            s.session_type == SessionType::Merge
                && s.stage_id.as_deref() == Some(stage.id.as_str())
                && is_live(s)
        })
        .max_by_key(|s| s.created_at)
}

/// The session that speaks for a stage whose `stage.session` names `named`.
///
/// Once the stage reaches a merge state its worker has finished: `stage.session`
/// still names it, as a session that has ended. The live resolver speaks for
/// the stage then, so the terminal gate, the pid and the activity describe the
/// session a person can attach to. A live `named` session always speaks for
/// itself.
pub(super) fn speaking_for<'a>(
    stage: &Stage,
    named: &'a Session,
    sessions: &'a [Session],
) -> &'a Session {
    if is_live(named) || !in_merge_state(&stage.status) {
        return named;
    }
    live_merge_resolver(stage, sessions).unwrap_or(named)
}

/// The live resolver's id and the resolver attempts spent, for a stage the
/// daemon resolves a merge for; `(None, None)` for every other stage. An id
/// that is unsafe as a path segment reads no counter file.
pub(super) fn merge_resolver_facts(
    stage: &Stage,
    sessions: &[Session],
    work_dir: &WorkDir,
) -> (Option<String>, Option<u32>) {
    if !in_merge_state(&stage.status) {
        return (None, None);
    }
    (
        live_merge_resolver(stage, sessions).map(|s| s.id.clone()),
        valid_stage_id(&stage.id).then(|| merge_resolver_attempts(work_dir.root(), &stage.id)),
    )
}
