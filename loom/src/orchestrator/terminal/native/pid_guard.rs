//! PID-identity liveness and teardown shared by every terminal lane.

use anyhow::{bail, Result};
use std::path::Path;

use crate::models::session::Session;
use crate::process::{terminate_verified, verify_process_identity, IdentityStatus};

use super::{read_pid_entry, NativeBackend};

/// Identity evidence available for a persisted terminal session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionProcessStatus {
    VerifiedAlive,
    Dead,
    Unverifiable,
    Missing,
}

/// Resolve liveness from the per-session PID identity file only.
///
/// `session.pid` is informational and is never an identity fallback. PID-file
/// evidence is retained for later diagnosis and safe teardown decisions.
pub(crate) fn session_process_status(work_dir: &Path, session: &Session) -> SessionProcessStatus {
    let Some((_, pid_key)) = NativeBackend::window_title_and_pid_key(session) else {
        return SessionProcessStatus::Missing;
    };
    let Some(identity) = read_pid_entry(work_dir, &pid_key) else {
        return SessionProcessStatus::Missing;
    };

    match verify_process_identity(identity) {
        IdentityStatus::VerifiedAlive => SessionProcessStatus::VerifiedAlive,
        IdentityStatus::Dead => SessionProcessStatus::Dead,
        IdentityStatus::Unverifiable => SessionProcessStatus::Unverifiable,
    }
}

pub(crate) fn pid_only_is_alive(work_dir: &Path, session: &Session) -> bool {
    matches!(
        session_process_status(work_dir, session),
        SessionProcessStatus::VerifiedAlive | SessionProcessStatus::Unverifiable
    )
}

/// Terminate a session only when its persisted PID and start-time both match.
///
/// Dead/mismatched evidence is a no-op and is retained as a tombstone. Missing
/// or unverifiable evidence returns an error and never falls back to raw PID.
/// A successful signal also retains the verified entry: SIGTERM is
/// asynchronous, so liveness confirmation still needs the PID plus start time
/// until it observes definitive death.
pub(crate) fn pid_only_terminate(work_dir: &Path, session: &Session) -> Result<()> {
    let Some((_, pid_key)) = NativeBackend::window_title_and_pid_key(session) else {
        bail!(
            "refusing to terminate session {} without a tracking key",
            session.id
        );
    };
    let Some(identity) = read_pid_entry(work_dir, &pid_key) else {
        bail!(
            "refusing to terminate session {} without PID identity evidence",
            session.id
        );
    };

    match verify_process_identity(identity) {
        IdentityStatus::Dead => Ok(()),
        IdentityStatus::Unverifiable => terminate_verified(identity).map(|_| ()),
        IdentityStatus::VerifiedAlive => {
            terminate_verified(identity)?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::pid_tracking::{create_pid_dir, pid_file_path};
    use super::*;
    use crate::process::ProcessIdentity;
    use tempfile::TempDir;

    fn assigned_session(stage_id: &str, pid: Option<u32>) -> Session {
        let mut session = Session::new();
        session.assign_to_stage(stage_id.to_string());
        session.pid = pid;
        session
    }

    fn write_identity(work_dir: &Path, session: &Session, identity: ProcessIdentity) {
        let (_, pid_key) = NativeBackend::window_title_and_pid_key(session).unwrap();
        create_pid_dir(work_dir).unwrap();
        let value = match identity.start_time {
            Some(start) => format!("{}\n{start}\n", identity.pid),
            None => format!("{}\n", identity.pid),
        };
        std::fs::write(pid_file_path(work_dir, &pid_key), value).unwrap();
    }

    #[test]
    fn missing_identity_never_falls_back_to_live_session_pid() {
        let temp = TempDir::new().unwrap();
        let session = assigned_session("missing-evidence", Some(std::process::id()));

        assert_eq!(
            session_process_status(temp.path(), &session),
            SessionProcessStatus::Missing
        );
        assert!(pid_only_terminate(temp.path(), &session).is_err());
    }

    #[test]
    fn missing_start_time_is_unverifiable_and_preserved() {
        let temp = TempDir::new().unwrap();
        let session = assigned_session("unverifiable", Some(std::process::id()));
        write_identity(
            temp.path(),
            &session,
            ProcessIdentity {
                pid: std::process::id(),
                start_time: None,
            },
        );
        let (_, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        let path = pid_file_path(temp.path(), &pid_key);

        assert_eq!(
            session_process_status(temp.path(), &session),
            SessionProcessStatus::Unverifiable
        );
        assert!(pid_only_terminate(temp.path(), &session).is_err());
        assert!(path.exists(), "unverifiable evidence must remain intact");
    }

    #[test]
    fn dead_identity_is_definitive_and_retained_as_a_tombstone() {
        let temp = TempDir::new().unwrap();
        let session = assigned_session("dead", Some(std::process::id()));
        write_identity(
            temp.path(),
            &session,
            ProcessIdentity {
                pid: 999_999_999,
                start_time: Some(1),
            },
        );
        let (_, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        let path = pid_file_path(temp.path(), &pid_key);

        assert_eq!(
            session_process_status(temp.path(), &session),
            SessionProcessStatus::Dead
        );
        pid_only_terminate(temp.path(), &session).unwrap();
        assert!(
            path.exists(),
            "dead identity is diagnostic tombstone evidence"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn matching_pid_and_start_time_are_the_only_kill_target() {
        let temp = TempDir::new().unwrap();
        let session = assigned_session("verified", Some(4242));
        let identity = ProcessIdentity {
            pid: std::process::id(),
            start_time: crate::process::process_start_time(std::process::id()),
        };
        write_identity(temp.path(), &session, identity);

        assert_eq!(
            session_process_status(temp.path(), &session),
            SessionProcessStatus::VerifiedAlive
        );
    }
}
