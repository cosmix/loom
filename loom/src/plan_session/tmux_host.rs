//! The tmux-backed [`SessionHost`] the bridge runs against in production.

use crate::orchestrator::terminal::tmux::teardown_socket;
use crate::plan_session::bridge::host::{LaunchSpec, SessionHost, Spawned};
use crate::plan_session::{launch, tmux_io};
use crate::process::{verify_process_identity, IdentityStatus, ProcessIdentity};

/// Runs sessions in tmux.
#[derive(Default, Clone, Copy, Debug)]
pub struct TmuxHost;

impl SessionHost for TmuxHost {
    fn spawn(&self, spec: &LaunchSpec) -> anyhow::Result<Spawned> {
        launch::launch(spec)
    }

    fn deliver(&self, socket: &str, target: &str, text: &str) -> anyhow::Result<()> {
        tmux_io::paste_text(socket, target, text)
    }

    fn interrupt(&self, socket: &str, target: &str) -> anyhow::Result<()> {
        tmux_io::press(socket, target, "Escape")
    }

    fn teardown(&self, socket: &str) -> anyhow::Result<()> {
        teardown_socket(socket)
    }

    fn identity(&self, identity: ProcessIdentity) -> IdentityStatus {
        verify_process_identity(identity)
    }
}
