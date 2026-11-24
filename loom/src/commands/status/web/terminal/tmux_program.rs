//! The `tmux` program the browser terminal runs, for its readiness probe and
//! its attach client.
//!
//! Production always runs `tmux` from `PATH`. Test builds can point both uses
//! at a stand-in executable through [`OverrideGuard`], so a test can make
//! target resolution succeed and the attach spawn fail without editing the
//! process-wide `PATH`, which every other test's program lookups share.

#[cfg(not(test))]
pub(super) fn command() -> std::process::Command {
    std::process::Command::new("tmux")
}

/// Readiness of the session's tmux endpoint, the gate `resolve_target` applies
/// before an attach is started.
#[cfg(not(test))]
pub(super) fn endpoint_ready(
    session: &crate::models::session::Session,
    tmux_session: &str,
) -> bool {
    crate::orchestrator::terminal::tmux::viewer::endpoint_ready(session, tmux_session)
}

#[cfg(test)]
pub(super) use seam::{command, endpoint_ready, OverrideGuard};

#[cfg(test)]
mod seam {
    use std::ffi::OsString;
    use std::process::{Command, Stdio};
    use std::sync::Mutex;

    use crate::models::session::Session;
    use crate::orchestrator::terminal::tmux::socket_name;

    /// A static, because the server's connection threads run the lookups: a
    /// thread-local set by the test thread would not reach them. Only the
    /// `#[serial]` test that holds an [`OverrideGuard`] sets it.
    static PROGRAM: Mutex<Option<OsString>> = Mutex::new(None);

    fn overridden() -> Option<OsString> {
        PROGRAM.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Routes this module's tmux program to `program` until dropped.
    pub(in crate::commands::status::web) struct OverrideGuard;

    impl OverrideGuard {
        pub(in crate::commands::status::web) fn set(program: impl Into<OsString>) -> Self {
            *PROGRAM.lock().unwrap_or_else(|e| e.into_inner()) = Some(program.into());
            Self
        }
    }

    impl Drop for OverrideGuard {
        fn drop(&mut self) {
            *PROGRAM.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }

    pub(in crate::commands::status::web) fn command() -> Command {
        Command::new(overridden().unwrap_or_else(|| "tmux".into()))
    }

    /// Without an override this is the production probe. With one, the
    /// override program answers `has-session` for the session's socket, since
    /// the socket file a test creates is not a live tmux server.
    pub(in crate::commands::status::web) fn endpoint_ready(
        session: &Session,
        tmux_session: &str,
    ) -> bool {
        let Some(program) = overridden() else {
            return crate::orchestrator::terminal::tmux::viewer::endpoint_ready(
                session,
                tmux_session,
            );
        };
        Command::new(program)
            .args([
                "-L",
                &socket_name(session),
                "has-session",
                "-t",
                tmux_session,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
}
