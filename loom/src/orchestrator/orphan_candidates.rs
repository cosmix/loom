//! Per-pid-file orphan candidate evaluation for `stage_candidates`.
//!
//! Split out of `session_registry.rs` to keep that file at its 400-line
//! ceiling. Behavior is unchanged from before the move.

use std::collections::HashSet;
use std::path::Path;
use std::time::SystemTime;

use crate::models::session::{Session, SessionBackendKind, SessionType};
use crate::process::{verify_process_identity, IdentityStatus};

use crate::orchestrator::terminal::native::read_pid_entry;
use crate::orchestrator::terminal::tmux::socket_path_for;

use super::OrphanEvidence;

/// Whether `stem` (a `.loom/work/pids/*.pid` file stem) names a live, unrecorded
/// `kind` agent for `stage_id`, and if so, the orphan evidence to adopt
/// paired with the pid file's mtime.
pub(super) fn orphan_candidate(
    work_dir: &Path,
    stage_id: &str,
    kind: SessionType,
    stem: &str,
    mtime: SystemTime,
    claimed: &HashSet<String>,
) -> Option<(OrphanEvidence, SystemTime)> {
    let tracking_key = Session::derive_tracking_key(stage_id, kind);
    let prefix = format!("{tracking_key}-");
    let session_id = stem.strip_prefix(&prefix)?;
    if !is_adoptable_session_id(work_dir, session_id, claimed) {
        return None;
    }

    let identity = read_pid_entry(work_dir, stem)?;
    // Exactly `pid_only_is_alive`'s rule, so an adopted record's
    // liveness answer matches the one every other caller computes.
    if !matches!(
        verify_process_identity(identity),
        IdentityStatus::VerifiedAlive | IdentityStatus::Unverifiable
    ) {
        return None;
    }

    let backend = if socket_path_for(&format!("loom-{session_id}")).exists() {
        SessionBackendKind::Tmux
    } else {
        SessionBackendKind::Native
    };

    Some((
        OrphanEvidence {
            session_id: session_id.to_string(),
            stage_id: stage_id.to_string(),
            tracking_key,
            session_type: kind,
            pid: identity.pid,
            backend,
        },
        mtime,
    ))
}

/// Whether `session_id` (parsed from a pid-file stem) is a real, unclaimed,
/// unrecorded session id worth probing for liveness.
fn is_adoptable_session_id(work_dir: &Path, session_id: &str, claimed: &HashSet<String>) -> bool {
    // Tracking keys are not self-delimiting: stage `a-b`'s pid file
    // `loom-a-b-session-X.pid` also starts with stage `a`'s prefix
    // `loom-a-`, yielding the nonexistent session id `b-session-X`.
    // Every id `Session::generate_id` makes is `session-<uuid8>-<ts>`,
    // so that shape rejects the mis-split and nothing loom can create.
    if !session_id.starts_with("session-") || claimed.contains(session_id) {
        return false;
    }
    // A pid file WITH a record is not an orphan. This is also what
    // makes adoption idempotent: the record `adopt_orphan` writes makes
    // the next scan skip right here.
    !work_dir
        .join("sessions")
        .join(format!("{session_id}.md"))
        .exists()
}
