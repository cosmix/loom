//! Login preflight for `loom run`.
//!
//! Stage sessions start `claude` under a minimal environment, not the
//! operator's. A login the operator's shell can see but that environment
//! cannot (a Keychain entry looked up by `$USER`, for example) makes every
//! session start "Not logged in" and burn its retry budget, so `loom run`
//! probes `claude auth status --json` under the stage environment first and
//! refuses a run that cannot work.

use anyhow::{bail, Result};
use std::path::Path;

use crate::claude::auth::{operator_auth_status, AuthProbe};
use crate::process::AGENT_SESSION_ENV_NAMES;

/// The refusal text for a probe pair, or `None` when the run may proceed. Only
/// a stage-environment `NotLoggedIn` refuses; the operator probe says why.
pub(super) fn auth_problem(stage: &AuthProbe, operator: &AuthProbe) -> Option<String> {
    if *stage != AuthProbe::NotLoggedIn {
        return None;
    }
    Some(match operator {
        AuthProbe::LoggedIn { .. } => format!(
            "claude is logged in for your shell but not under the environment stage sessions \
             receive (HOME, PATH and {}), so stage sessions would start \"Not logged in\"",
            AGENT_SESSION_ENV_NAMES.join(", ")
        ),
        AuthProbe::NotLoggedIn => {
            "claude is not logged in; run claude /login, then loom run again".to_string()
        }
        AuthProbe::Unknown(reason) => format!(
            "claude is not logged in under the environment stage sessions receive, and the \
             check of your own shell's login was inconclusive ({reason}); run claude /login, \
             then loom run again"
        ),
    })
}

/// The decision behind [`require_stage_login`]: a logged-in stage probe passes,
/// an inconclusive one warns and passes, and a logged-out one refuses with the
/// reason `operator` (run only then) gives.
pub(super) fn require_login_with(
    stage: AuthProbe,
    operator: impl FnOnce() -> AuthProbe,
) -> Result<()> {
    match &stage {
        AuthProbe::LoggedIn { .. } => Ok(()),
        AuthProbe::Unknown(reason) => {
            eprintln!("could not verify the claude login for stage sessions: {reason}");
            Ok(())
        }
        AuthProbe::NotLoggedIn => match auth_problem(&stage, &operator()) {
            Some(problem) => bail!("{problem}"),
            None => Ok(()),
        },
    }
}

/// Refuse to start when claude is logged out under the stage environment. The
/// probe is memoized by `remote_control::cached_stage_auth`, so the Remote
/// Control preflight that follows reuses this run's result.
pub(super) fn require_stage_login(claude_path: &Path) -> Result<()> {
    require_login_with(
        crate::remote_control::cached_stage_auth(claude_path).clone(),
        || operator_auth_status(claude_path),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logged_in() -> AuthProbe {
        AuthProbe::LoggedIn {
            method: "claude.ai".to_string(),
        }
    }

    fn unknown() -> AuthProbe {
        AuthProbe::Unknown("timed out after 30s".to_string())
    }

    #[test]
    fn auth_problem_is_none_unless_the_stage_probe_is_logged_out() {
        for stage in [logged_in(), unknown()] {
            for operator in [logged_in(), AuthProbe::NotLoggedIn, unknown()] {
                assert_eq!(auth_problem(&stage, &operator), None);
            }
        }
    }

    #[test]
    fn auth_problem_names_the_stage_environment_when_only_it_is_logged_out() {
        let problem = auth_problem(&AuthProbe::NotLoggedIn, &logged_in()).unwrap();
        assert!(problem.contains("stage sessions"), "{problem}");
        assert!(problem.contains("USER"), "{problem}");
        assert!(!problem.contains("claude /login"), "{problem}");
    }

    #[test]
    fn auth_problem_asks_for_a_login_when_the_operator_is_logged_out_too() {
        for operator in [AuthProbe::NotLoggedIn, unknown()] {
            let problem = auth_problem(&AuthProbe::NotLoggedIn, &operator).unwrap();
            assert!(problem.contains("claude /login"), "{problem}");
        }
    }

    #[test]
    fn not_logged_in_refuses_the_run() {
        let error = require_login_with(AuthProbe::NotLoggedIn, || AuthProbe::NotLoggedIn)
            .unwrap_err()
            .to_string();
        assert!(error.contains("claude /login"), "{error}");
    }

    #[test]
    fn logged_in_and_unknown_probes_pass_without_asking_the_operator() {
        let never = || -> AuthProbe { panic!("operator probe must not run") };
        assert!(require_login_with(logged_in(), never).is_ok());
        assert!(require_login_with(unknown(), never).is_ok());
    }

    #[test]
    fn require_stage_login_warns_and_passes_when_claude_cannot_run() {
        // An inconclusive stage probe (claude could not run) warns and passes.
        // Constructed probes keep this off the process-global stage-auth cache.
        let never = || -> AuthProbe { panic!("operator probe must not run") };
        let cannot_run = AuthProbe::Unknown("claude could not be started".to_string());
        assert!(require_login_with(cannot_run, never).is_ok());
    }

    #[test]
    fn an_inconclusive_operator_probe_is_not_reported_as_a_logged_out_shell() {
        let problem = auth_problem(&AuthProbe::NotLoggedIn, &unknown()).unwrap();
        assert!(problem.contains("inconclusive"), "{problem}");
        assert!(problem.contains("timed out after 30s"), "{problem}");
        assert!(problem.contains("claude /login"), "{problem}");
    }
}
