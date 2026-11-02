//! Shared attribution and lifecycle policy for cleanup of tmux sockets.
use super::{kill_socket_server, list_loom_sockets, socket_session_is_alive, LoomSocket};
use anyhow::{bail, Result};
use std::{fs, io, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionReapMode {
    OrphansOnly,
    IncludeLiveBeforeClean,
}

#[derive(Debug, Default)]
pub struct ReapStats {
    pub orphaned: usize,
    pub live: usize,
    pub unattributed: usize,
}

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Unattributed,
    Kept,
    Reaped { alive: bool },
}

fn reap_one(
    socket: &LoomSocket,
    mode: SessionReapMode,
    alive: impl FnOnce() -> bool,
    kill: impl FnOnce() -> bool,
    remove: impl FnOnce() -> io::Result<()>,
) -> Result<Outcome> {
    if !socket.attributed {
        return Ok(Outcome::Unattributed);
    }
    let alive = alive();
    if alive && mode == SessionReapMode::OrphansOnly {
        return Ok(Outcome::Kept);
    }
    if !kill() && alive {
        bail!(
            "kill-server failed for live session {}; socket retained at {}",
            socket.session_id,
            socket.path.display()
        );
    }
    if let Err(error) = remove() {
        if error.kind() != io::ErrorKind::NotFound {
            bail!("could not remove socket {}: {error}", socket.path.display());
        }
    }
    Ok(Outcome::Reaped { alive })
}

/// Failure preserves the attribution state in callers that are about to delete it.
pub fn reap_sockets(work_dir: &Path, mode: SessionReapMode) -> Result<ReapStats> {
    let mut stats = ReapStats::default();
    let mut failures = Vec::new();
    for socket in list_loom_sockets(work_dir) {
        match reap_one(
            &socket,
            mode,
            || socket_session_is_alive(work_dir, &socket.session_id),
            || kill_socket_server(&socket.path),
            || fs::remove_file(&socket.path),
        ) {
            Ok(Outcome::Unattributed) => stats.unattributed += 1,
            Ok(Outcome::Kept) => {}
            Ok(Outcome::Reaped { alive: true }) => stats.live += 1,
            Ok(Outcome::Reaped { alive: false }) => stats.orphaned += 1,
            Err(error) => failures.push(error.to_string()),
        }
    }
    if !failures.is_empty() {
        bail!("tmux cleanup failed: {}", failures.join("; "));
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::path::PathBuf;
    #[test]
    fn policy_covers_attribution_liveness_and_failures() {
        let mut socket = LoomSocket {
            path: PathBuf::from("socket"),
            session_id: "s".into(),
            attributed: false,
        };
        let touched = Cell::new(false);
        let outcome = reap_one(
            &socket,
            SessionReapMode::IncludeLiveBeforeClean,
            || panic!("unattributed"),
            || true,
            || Ok(()),
        )
        .unwrap();
        assert_eq!(outcome, Outcome::Unattributed);
        socket.attributed = true;
        let outcome = reap_one(
            &socket,
            SessionReapMode::OrphansOnly,
            || true,
            || {
                touched.set(true);
                true
            },
            || Ok(()),
        )
        .unwrap();
        assert_eq!(outcome, Outcome::Kept);
        assert!(!touched.get());
    }

    #[test]
    fn attributed_sockets_reap_and_preserve_failures() {
        let socket = LoomSocket {
            path: PathBuf::from("socket"),
            session_id: "s".into(),
            attributed: true,
        };
        for alive in [false, true] {
            let outcome = reap_one(
                &socket,
                SessionReapMode::IncludeLiveBeforeClean,
                || alive,
                || true,
                || Ok(()),
            )
            .unwrap();
            assert_eq!(outcome, Outcome::Reaped { alive });
        }
        assert!(reap_one(
            &socket,
            SessionReapMode::IncludeLiveBeforeClean,
            || true,
            || false,
            || panic!("live socket must remain")
        )
        .is_err());
        assert!(reap_one(
            &socket,
            SessionReapMode::OrphansOnly,
            || false,
            || false,
            || Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
        )
        .is_err());
    }
}
