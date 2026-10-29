//! Per-session sandbox settings builder for planning sessions.
//!
//! Writes section 12's `settings.json` ("The settings file", "Pressure sessions", "Modes per
//! kind") and the guard's `guard.json` into a session directory. It is a builder beside
//! `sandbox::settings`, never `generate_settings_json`, which adds the package-manager cache
//! writes, the orchestration-state reads and plan-authored config a planning session must not
//! have. Layer 1 is `filesystem` and [`read_denies`], layer 2 and the hooks `permissions`;
//! layer 3 is the guard (`plan_session::guard`) the `PreToolUse` hook runs.

mod assert;
pub(crate) mod filesystem;
mod permissions;
mod reads;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_assert;
#[cfg(test)]
mod tests_features;
#[cfg(test)]
mod tests_kinds;
#[cfg(test)]
mod tests_kinds_fixture;

use std::borrow::Cow;
use std::fs::File;
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use shell_escape::escape;

pub use assert::assert_planning_settings;
pub use reads::{read_denies, ReadDenies};

use crate::fs::safe_fs::{
    open_safely, safe_create_dir_all, safe_create_dir_all_in_workdir, safe_open_dirfd,
    safe_write_with_mode_in_workdir,
};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::planning::paths::LockedSet;
use crate::planning::runtime::{create_runtime_dir, RuntimePaths};

/// The file tools the planning guard decides.
pub const PRE_TOOL_USE_MATCHER: &str = "Read|Edit|Write|MultiEdit|NotebookEdit|Grep|Glob";
/// Seconds Claude Code waits for the guard.
pub const PRE_TOOL_USE_TIMEOUT_SECS: u32 = 10;
/// The hook event plan 6 owns; an extra hook for it is refused.
pub(crate) const PRE_TOOL_USE: &str = "PreToolUse";
/// `settings.json` and `guard.json` are the host's, readable by the user alone.
const SANDBOX_FILE_MODE: libc::mode_t = 0o600;
/// The runtime grant directories and a pressure session's `done/`.
const PRIVATE_DIR_MODE: libc::mode_t = 0o700;

/// Everything a session's sandbox files are built from.
#[derive(Debug, Clone, Copy)]
pub struct SandboxInputs<'a> {
    /// The project root, canonical and absolute.
    pub project_root: &'a Path,
    pub runtime: &'a RuntimePaths,
    pub session_dir: &'a SessionDir,
    pub kind: &'a SessionKind,
    /// Absolute path of the running loom.
    pub loom_bin: &'a Path,
    /// The user's home, absolute.
    pub home: &'a Path,
}

/// One hook registration beside the guard's `PreToolUse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookSpec {
    pub event: &'static str,
    pub matcher: Option<&'static str>,
    pub timeout_secs: Option<u32>,
}

/// The absolute paths `write_session_sandbox` wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxFiles {
    pub settings: PathBuf,
    pub guard: PathBuf,
}

/// The session kind with a pressure session's plan and bundle validated.
enum Target {
    Authoring,
    Summary,
    /// `plan` is `<home>/PLAN-<slug>.md`, `bundle` is `<home>/briefs/<slug>`.
    Pressure {
        plan: PathBuf,
        bundle: PathBuf,
    },
}

/// One settings build: the inputs, the tree listing and the locked set it was scanned into.
struct Build<'a> {
    inputs: &'a SandboxInputs<'a>,
    locked: &'a LockedSet,
    tree: filesystem::Tree,
    target: Target,
    /// The session's `done/`, a pressure session's only write grant.
    done: PathBuf,
}

/// The hook command line `loom plan hook <event> --session-dir <dir>`, each path shell-escaped
/// once (Claude Code runs it through a shell).
pub fn hook_command(loom_bin: &Path, event: &str, session_dir: &Path) -> String {
    let bin = escape(loom_bin.to_string_lossy()).into_owned();
    let event = escape(Cow::Borrowed(event)).into_owned();
    let dir = escape(session_dir.to_string_lossy()).into_owned();
    format!("{bin} plan hook {event} --session-dir {dir}")
}

/// The settings object of the session `inputs` describes, over the locked set `locked`, with
/// `extra_hooks` registered after the guard's `PreToolUse` in order.
pub fn build_settings(
    inputs: &SandboxInputs<'_>,
    locked: &LockedSet,
    extra_hooks: &[HookSpec],
) -> Result<Value> {
    check_paths(inputs)?;
    let build = Build {
        inputs,
        locked,
        tree: filesystem::Tree::open(inputs.project_root)?,
        target: target(inputs.kind)?,
        done: inputs.session_dir.done(),
    };
    let (deny_write, allow_write) = filesystem::write_lists(&build)?;
    let reads = read_denies(inputs.home, inputs.project_root, &inputs.runtime.dir)?;
    let mut allow_read = reads.carve_outs;
    let pressure = matches!(build.target, Target::Pressure { .. });
    if pressure {
        allow_read.push(filesystem::text(&build.done)?);
    }
    let mut settings = json!({
        "sandbox": {
            "enabled": true,
            "failIfUnavailable": true,
            "autoAllowBashIfSandboxed": false,
            "allowUnsandboxedCommands": false,
            "network": { "strictAllowlist": true },
            "filesystem": {
                "denyWrite": deny_write,
                "allowWrite": allow_write,
                "denyRead": reads.denied,
                "allowRead": allow_read,
            },
        },
        "permissions": permissions::permissions(&build)?,
        "worktree": { "bgIsolation": "none" },
        "hooks": permissions::hooks(&build, extra_hooks)?,
    });
    if pressure {
        // `/pressure` uses an agent team: teammates stay inside the session's own process, so
        // they open no tmux pane the bridge does not watch (section 11). A top-level key; Claude
        // Code does not read it inside `permissions`.
        settings["teammateMode"] = json!("in-process");
    }
    Ok(settings)
}

/// Create the runtime directory and its grants: `drafts/` and `claims/` (0700) and an empty
/// `summaries.lock` (0600) when absent. An existing lock file is never truncated or locked.
pub fn ensure_runtime_grants(runtime: &RuntimePaths) -> Result<()> {
    create_runtime_dir(runtime)?;
    for dir in [&runtime.drafts, &runtime.claims] {
        safe_create_dir_all(&runtime.dir, relative(&runtime.dir, dir)?, PRIVATE_DIR_MODE)
            .with_context(|| format!("cannot create {}", dir.display()))?;
    }
    let lock = &runtime.summaries_lock;
    let dirfd = safe_open_dirfd(&runtime.dir)
        .with_context(|| format!("cannot open {}", runtime.dir.display()))?;
    // O_CREAT without O_TRUNC leaves an existing lock byte-identical, and no flock is taken, so
    // a summary write holding the lock never blocks a launch; O_NONBLOCK keeps a planted FIFO
    // from blocking the open, and the type check below refuses it.
    let flags = libc::O_RDONLY | libc::O_CREAT | libc::O_NONBLOCK;
    let fd = open_safely(
        dirfd.as_raw_fd(),
        relative(&runtime.dir, lock)?,
        flags,
        0o600,
    )
    .with_context(|| format!("cannot create {}", lock.display()))?;
    let meta = File::from(fd)
        .metadata()
        .with_context(|| format!("cannot inspect {}", lock.display()))?;
    if !meta.is_file() {
        bail!("{} is not a regular file", lock.display());
    }
    Ok(())
}

/// Write `settings.json` and `guard.json` (0600, no-follow) into the session directory, after
/// ensuring the runtime grants, creating a pressure session's `done/`, scanning the locked set
/// and asserting the settings.
pub fn write_session_sandbox(
    inputs: &SandboxInputs<'_>,
    extra_hooks: &[HookSpec],
) -> Result<SandboxFiles> {
    ensure_runtime_grants(inputs.runtime)?;
    let session = inputs.session_dir.path();
    let dirfd = safe_open_dirfd(session)
        .with_context(|| format!("cannot open the session directory {}", session.display()))?;
    if matches!(inputs.kind, SessionKind::Pressure { .. }) {
        let done = inputs.session_dir.done();
        let rel = relative(session, &done)?;
        safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), rel, PRIVATE_DIR_MODE)
            .with_context(|| format!("cannot create {}", done.display()))?;
    }
    let locked = LockedSet::scan(inputs.project_root)?;
    let settings = build_settings(inputs, &locked, extra_hooks)?;
    assert_planning_settings(&settings)?;
    // GuardPolicy (`plan_session::guard`), written from its exact fields so this module does not
    // depend on the guard; `build_settings` refused every non-UTF-8 path above.
    let guard = json!({
        "schema": 1,
        "kind": inputs.kind,
        "project_root": inputs.project_root,
        "runtime_dir": inputs.runtime.dir,
        "session_dir": session,
        "home": inputs.home,
    });
    let files = SandboxFiles {
        settings: inputs.session_dir.settings(),
        guard: inputs.session_dir.guard(),
    };
    write_private(&dirfd, session, &files.settings, &settings)?;
    write_private(&dirfd, session, &files.guard, &guard)?;
    Ok(files)
}

/// Write `value` as pretty JSON to `path` under the session directory `session`.
fn write_private(dirfd: &OwnedFd, session: &Path, path: &Path, value: &Value) -> Result<()> {
    let body = serde_json::to_string_pretty(value).context("cannot serialise a sandbox file")?;
    let rel = relative(session, path)?;
    safe_write_with_mode_in_workdir(dirfd.as_raw_fd(), rel, body.as_bytes(), SANDBOX_FILE_MODE)
        .with_context(|| format!("cannot write {}", path.display()))
}

fn relative<'p>(base: &Path, path: &'p Path) -> Result<&'p Path> {
    path.strip_prefix(base)
        .with_context(|| format!("{} is not under {}", path.display(), base.display()))
}

/// Every path a rule or list is built from must be absolute and UTF-8: a relative one would
/// resolve elsewhere, and JSON cannot carry a non-UTF-8 one exactly.
fn check_paths(inputs: &SandboxInputs<'_>) -> Result<()> {
    // These three are interpolated into permission rules; a rule character in them would make
    // a rule match a sibling path and miss the real one. `loom_bin` is shell-escaped into the
    // hook command instead, and `home` only appears in `~/` rules and the guard policy.
    let in_rules = [
        ("project root", inputs.project_root),
        ("runtime directory", inputs.runtime.dir.as_path()),
        ("session directory", inputs.session_dir.path()),
    ];
    let others = [("loom binary", inputs.loom_bin), ("home", inputs.home)];
    for (what, path) in in_rules.iter().chain(&others) {
        if !path.is_absolute() || path.to_str().is_none() {
            bail!(
                "the {what} {} must be an absolute UTF-8 path",
                path.display()
            );
        }
    }
    for (what, path) in in_rules {
        let chars = permissions::rule_syntax_chars(&path.to_string_lossy());
        if !chars.is_empty() {
            bail!(
                "the {what} {} holds permission-rule syntax characters {chars:?}",
                path.display()
            );
        }
    }
    Ok(())
}

fn target(kind: &SessionKind) -> Result<Target> {
    Ok(match kind {
        SessionKind::Authoring => Target::Authoring,
        SessionKind::Summary => Target::Summary,
        SessionKind::Pressure { plan } => {
            let Some(bundle) = kind.pressure_bundle() else {
                bail!(
                    "a pressure session needs a PLAN-<slug>.md in a valid plan home, got {}",
                    plan.display()
                );
            };
            Target::Pressure {
                plan: plan.clone(),
                bundle,
            }
        }
    })
}
