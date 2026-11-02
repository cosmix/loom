//! Per-pid-file orphan candidate evaluation for `stage_candidates`.
//!
//! Match session identity and process evidence before adopting an orphan.

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
    legacy: bool,
) -> Option<(OrphanEvidence, SystemTime)> {
    let tracking_key = if legacy {
        Session::legacy_tracking_key(stage_id, kind)
    } else {
        Session::derive_tracking_key(stage_id, kind)
    };
    let prefix = format!("{tracking_key}-");
    let session_id = stem.strip_prefix(&prefix)?;
    if !is_adoptable_session_id(work_dir, session_id, claimed) {
        return None;
    }

    if !owner_matches(work_dir, stem, &tracking_key, legacy) {
        return None;
    }
    let identity = read_pid_entry(work_dir, stem)?;
    // Match pid_only_is_alive's liveness rule.
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
    let Some((uuid, timestamp)) = session_id
        .strip_prefix("session-")
        .and_then(|id| id.split_once('-'))
    else {
        return false;
    };
    if uuid.len() != 8
        || !uuid.bytes().all(|b| b.is_ascii_hexdigit())
        || timestamp.is_empty()
        || !timestamp.bytes().all(|b| b.is_ascii_digit())
        || claimed.contains(session_id)
    {
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

fn owner_matches(work_dir: &Path, stem: &str, tracking_key: &str, legacy: bool) -> bool {
    let contents = std::fs::read_to_string(work_dir.join("pids").join(format!("{stem}.pid"))).ok();
    let Some(contents) = contents else {
        return false;
    };
    let owner = contents
        .lines()
        .nth(2)
        .and_then(|line| line.strip_prefix("loom-owner:"));
    if legacy {
        // Every prefixed legacy key can also be a regular stage's key.
        let ambiguous = [
            "v2-",
            "merge-",
            "base-conflict-",
            "knowledge-",
            "adjudication-",
            "contract-",
        ]
        .iter()
        .any(|prefix| {
            tracking_key
                .strip_prefix("loom-")
                .is_some_and(|s| s.starts_with(prefix))
        });
        if owner.is_some() || ambiguous {
            return false;
        }
    } else if owner != Some(tracking_key) {
        return false;
    }

    true
}
