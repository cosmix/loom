//! The per-session settings capsule every session kind launches from.
//!
//! [`write_session_capsule`] runs for Stage, Knowledge, Merge, BaseConflict
//! and Adjudication alike, from `prepare_session_launch`, which both terminal
//! lanes share. The capsule lives at
//! `<work_dir>/capsules/<session-id>.settings.json` (file 0600, directory
//! 0700) and is passed as `--settings <absolute path>` with
//! `--setting-sources user,project`, so no session loads a
//! `.claude/settings.local.json` any more. [`contents`] builds the document;
//! this module owns the file and its lifecycle.

mod contents;

use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use crate::fs::safe_fs;
use crate::models::session::SessionType;
use crate::sandbox::control_surfaces::{
    codex_plugin_entries, session_denies, ControlSurfaces, DenyInputs, SessionDenies,
};
use crate::sandbox::MergedSandboxConfig;
use crate::validation::validate_id;

/// The capsule directory's mode: private, like the rest of `.loom/work/`.
const CAPSULE_DIR_MODE: u32 = 0o700;
/// The capsule file's mode: it carries the session's permission rules.
const CAPSULE_FILE_MODE: u32 = 0o600;

/// `<work_dir>/capsules/` — where per-session generated settings files live.
fn capsules_dir(work_dir: &Path) -> PathBuf {
    work_dir.join("capsules")
}

/// The path a session's capsule is (or would be) written to.
///
/// `pub(crate)`, not `pub(super)`: `core/judge_close.rs` and
/// `core/event_handler/stalled_judge_tests.rs` need it too (via the
/// re-export in `native/mod.rs`).
pub(crate) fn session_settings_path(work_dir: &Path, session_id: &str) -> PathBuf {
    capsules_dir(work_dir).join(format!("{session_id}.settings.json"))
}

/// What [`write_session_capsule`] needs, resolved by the launch.
pub(super) struct CapsuleRequest<'a> {
    pub kind: SessionType,
    pub session_id: &'a str,
    /// The stage's merged, path-expanded sandbox config.
    pub sandbox: &'a MergedSandboxConfig,
    /// Where the session runs: a stage worktree or the repository root.
    pub cwd: &'a Path,
    pub work_dir: &'a Path,
    pub repo_root: &'a Path,
    /// The verified loom hooks directory; `None` refuses the spawn.
    pub hooks_dir: Option<&'a Path>,
    /// This session's own scratch directory, `<scratch_root>/<session-id>`.
    pub scratch_dir: &'a Path,
    /// What the approved-permissions list may never grant, and the
    /// executable directories and home directory the capsule's denies name.
    pub surfaces: &'a ControlSurfaces,
    /// Every root a session could write (`HostFacts::writable_roots`): an
    /// executable directory that is an ancestor of one is not denied, since
    /// denying it would deny the writable root it sits above too.
    pub writable_roots: &'a [PathBuf],
    /// The first `python3` on the pinned hook PATH (`HostFacts::python3`),
    /// the interpreter the capsule writes for a Python hook command.
    pub python3: Option<&'a Path>,
    /// Every regular file directly in the hooks directory whose first line
    /// is a python shebang (`HostFacts::python_hooks`).
    pub python_hooks: &'a [PathBuf],
}

/// Build and write the session's capsule, returning its absolute path.
///
/// A session with no verified hooks directory is refused, whatever its kind:
/// its capsule would register none of loom's hooks, the relay and the guards
/// included.
pub(super) fn write_session_capsule(request: &CapsuleRequest<'_>) -> Result<String> {
    validate_id(request.session_id).with_context(|| {
        format!(
            "invalid session id for a settings capsule: {}",
            request.session_id
        )
    })?;
    let hooks_dir = request
        .hooks_dir
        .ok_or_else(|| missing_hooks_dir(request))?;
    let state_root = super::wrapper::absolute(request.work_dir);
    let repo_root = super::wrapper::absolute(request.repo_root);
    let cwd = super::wrapper::absolute(request.cwd);
    let worktree_rooted = crate::sandbox::target_is_worktree(&cwd);
    let checkout_settings = read_checkout_settings(&repo_root)?;
    let approved = crate::fs::permissions::approved::approved_rules(&state_root, request.surfaces);
    let worktree = worktree_rooted.then_some(cwd.as_path());
    let denies = capsule_denies(request, hooks_dir, &repo_root, &state_root, worktree)?;
    let settings = contents::capsule_settings(&contents::CapsuleInputs {
        kind: request.kind,
        sandbox: request.sandbox,
        worktree_rooted,
        state_root: &state_root,
        repo_root: &repo_root,
        hooks_dir,
        scratch_dir: request.scratch_dir,
        approved: &approved,
        checkout_settings: checkout_settings.as_ref(),
        denies: &denies,
        python3: request.python3,
        python_hooks: request.python_hooks,
    })?;
    let path = write_capsule_file(request.work_dir, request.session_id, &settings)?;
    path.to_str().map(str::to_owned).with_context(|| {
        format!(
            "session settings capsule path is not valid UTF-8: {}",
            path.display()
        )
    })
}

/// The refusal of a spawn with no verified hooks directory, naming the one
/// `find_hooks_dir` found when that one failed verification.
fn missing_hooks_dir(request: &CapsuleRequest<'_>) -> anyhow::Error {
    let reason = match crate::hooks::find_hooks_dir() {
        Some(dir) => format!(
            "the loom hooks directory {} failed verification (it must resolve to a \
             directory the operator owns)",
            dir.display()
        ),
        None => "no loom hooks directory is installed ($LOOM_HOOKS_DIR, ~/.claude/hooks/loom)"
            .to_string(),
    };
    anyhow::anyhow!(
        "refusing to spawn {} session {}: {reason}; run `loom install-assets` and retry",
        request.kind,
        request.session_id
    )
}

/// The session's write denies (`sandbox::control_surfaces::session_denies`):
/// the verified hooks directory and every executable directory the control
/// surfaces name, and with the codex lane the `~/.claude/plugins` entries
/// beside its grant, listed now.
fn capsule_denies(
    request: &CapsuleRequest<'_>,
    hooks_dir: &Path,
    repo_root: &Path,
    state_root: &Path,
    worktree: Option<&Path>,
) -> Result<SessionDenies> {
    let plugin_entries = match request.surfaces.home() {
        Some(home) if request.sandbox.implementers.includes_codex() => {
            Some(codex_plugin_entries(home)?)
        }
        _ => None,
    };
    let mut executable_dirs = vec![hooks_dir.to_path_buf()];
    executable_dirs.extend_from_slice(request.surfaces.executable_dirs());
    session_denies(&DenyInputs {
        repo_root,
        state_root,
        worktree,
        executable_dirs: &executable_dirs,
        plugin_entries: plugin_entries.as_deref(),
        writable_roots: request.writable_roots,
    })
}

/// Read `<repo_root>/.claude/settings.local.json`, the checkout's own local
/// settings, whose deny rules and (codex lane) plugin keys the capsule
/// carries. A file that exists but does not parse is an error: a session must
/// never start under a capsule that silently dropped the operator's denies.
fn read_checkout_settings(repo_root: &Path) -> Result<Option<Value>> {
    let path = repo_root.join(".claude").join("settings.local.json");
    if !path.is_file() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let value = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {} as JSON", path.display()))?;
    Ok(Some(value))
}

/// Write `settings` as the session's capsule and return its absolute path.
///
/// The directory is created (or tightened) to 0700 and the file to 0600
/// through the dirfd-based helpers in [`crate::fs::safe_fs`], which refuse to
/// follow a symlink planted anywhere below `work_dir` — at `capsules` itself
/// or at the leaf settings file — before creating, `chmod`-ing or writing
/// through it. The returned path is absolute even for a relative `work_dir`:
/// the wrapper `cd`s into the session's working directory before `exec`ing
/// claude, so a relative `--settings` would resolve against the wrong root.
pub(super) fn write_capsule_file(
    work_dir: &Path,
    session_id: &str,
    settings: &Value,
) -> Result<PathBuf> {
    validate_id(session_id)
        .with_context(|| format!("invalid session id for a settings capsule: {session_id}"))?;
    let dir = capsules_dir(work_dir);
    fs::create_dir_all(work_dir)
        .with_context(|| format!("failed to create {}", work_dir.display()))?;
    let work_dirfd = safe_fs::safe_open_dirfd(work_dir)
        .with_context(|| format!("failed to open {}", work_dir.display()))?;
    #[allow(clippy::unnecessary_cast)] // mode_t is u32 on Linux but u16 on macOS
    let dir_mode = CAPSULE_DIR_MODE as libc::mode_t;
    safe_fs::safe_create_dir_all_in_workdir(
        work_dirfd.as_raw_fd(),
        Path::new("capsules"),
        dir_mode,
    )
    .with_context(|| format!("failed to create {}", dir.display()))?;
    tighten_existing_dir(&dir, CAPSULE_DIR_MODE)?;

    let content = serde_json::to_string_pretty(settings)
        .context("failed to serialize the session's settings capsule")?;
    let file_name = format!("{session_id}.settings.json");
    #[allow(clippy::unnecessary_cast)] // mode_t is u32 on Linux but u16 on macOS
    let file_mode = CAPSULE_FILE_MODE as libc::mode_t;
    safe_fs::safe_write_with_mode_in_workdir(
        work_dirfd.as_raw_fd(),
        &Path::new("capsules").join(&file_name),
        content.as_bytes(),
        file_mode,
    )
    .with_context(|| format!("failed to write {}", dir.join(&file_name).display()))?;

    let dir = dir
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", dir.display()))?;
    Ok(dir.join(file_name))
}

/// Tighten an already-existing real `dir` to `mode`: `safe_create_dir_all_in_workdir`
/// only sets the mode on a directory it creates, so one left over from an
/// earlier, more permissive version needs a second pass. Reopening it with
/// `O_NOFOLLOW` and `fchmod`-ing the resulting descriptor — never the path —
/// keeps this refusing a symlink swapped in between the two calls.
fn tighten_existing_dir(dir: &Path, mode: u32) -> Result<()> {
    let dir_fd = safe_fs::safe_open_dirfd(dir)
        .with_context(|| format!("failed to open {}", dir.display()))?;
    fs::File::from(dir_fd)
        .set_permissions(fs::Permissions::from_mode(mode))
        .with_context(|| format!("failed to restrict {}", dir.display()))
}

/// Remove a session's generated settings capsule, if any.
///
/// Best-effort, mirroring [`crate::orchestrator::monitor::heartbeat::cleanup_judge_heartbeat`]:
/// a missing file is not an error, since cleanup can race a session that
/// never got far enough to have one written.
pub(crate) fn cleanup_session_settings(work_dir: &Path, session_id: &str) {
    let path = session_settings_path(work_dir, session_id);
    if let Err(error) = fs::remove_file(&path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                session_id = %session_id,
                path = %path.display(),
                %error,
                "failed to remove the session's generated settings capsule",
            );
        }
    }
}

#[cfg(test)]
#[path = "tests_capsule_interpreters.rs"]
mod tests_capsule_interpreters;
#[cfg(test)]
#[path = "tests_capsule_contents.rs"]
mod tests_contents;
