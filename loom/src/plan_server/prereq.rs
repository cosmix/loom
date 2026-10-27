//! Detect prerequisites that make the planning server read-only.

use crate::claude::auth::{stage_auth_status, AuthProbe};
use crate::plan_server::wire::server::{MissingPrereq, PrereqKind};

#[derive(Clone, Debug)]
pub struct ReadOnly {
    problems: Vec<MissingPrereq>,
}

impl ReadOnly {
    pub fn detect() -> Self {
        let tmux = which::which("tmux").is_ok();
        let claude = crate::claude::find_claude_path()
            .ok()
            .map(|path| stage_auth_status(&path));
        #[cfg(target_os = "linux")]
        let sandbox = {
            let version = std::fs::read_to_string("/proc/version").ok();
            crate::commands::run::sandbox_preflight::sandbox_prerequisite_problem(
                version.as_deref(),
                which::which("bwrap").is_ok(),
                which::which("socat").is_ok(),
            )
        };
        #[cfg(not(target_os = "linux"))]
        let sandbox = None;
        Self::from_problems(collect_problems(tmux, claude, sandbox))
    }

    pub fn from_problems(problems: Vec<MissingPrereq>) -> Self {
        Self { problems }
    }

    pub fn is_read_only(&self) -> bool {
        !self.problems.is_empty()
    }

    pub fn problems(&self) -> &[MissingPrereq] {
        &self.problems
    }
}

fn problem(what: PrereqKind, message: impl Into<String>, fix: &str) -> MissingPrereq {
    MissingPrereq {
        what,
        message: message.into(),
        fix: fix.to_string(),
    }
}

pub(super) fn collect_problems(
    tmux: bool,
    claude: Option<AuthProbe>,
    sandbox: Option<String>,
) -> Vec<MissingPrereq> {
    let mut problems = Vec::new();
    if !tmux {
        problems.push(problem(
            PrereqKind::Tmux,
            "tmux is not on PATH",
            "Install tmux",
        ));
    }
    match claude {
        None => problems.push(problem(
            PrereqKind::Claude,
            "claude is not on PATH",
            "Install Claude Code",
        )),
        Some(AuthProbe::NotLoggedIn) => problems.push(problem(
            PrereqKind::ClaudeLogin,
            "Claude Code is not logged in",
            "Run claude auth login",
        )),
        Some(AuthProbe::Unknown(reason)) => problems.push(problem(
            PrereqKind::ClaudeLogin,
            format!("could not determine the Claude Code login ({reason})"),
            "Run claude auth login",
        )),
        Some(AuthProbe::LoggedIn { .. }) => {}
    }
    if let Some(message) = sandbox {
        problems.push(problem(
            PrereqKind::Sandbox,
            message,
            "Install bubblewrap and socat",
        ));
    }
    problems
}
