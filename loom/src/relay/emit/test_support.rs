//! Test-only fixture: a [`RelayContext`] valid for every guard in
//! [`RelayContext::check`] up to the verdict-matrix check, for any
//! `SessionType`, plus the cwd to call it with.
//!
//! Every guard needs real filesystem state: a scratch directory
//! [`crate::relay::scratch::validate_session_dir`] accepts, and — for the
//! "within checkout" guard — either a stage worktree or a project root
//! reachable through `work_dir`. Building that by hand in each relay test
//! duplicates the same `tempfile`/`fs::set_permissions` calls; this is the
//! one place that does it.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use tempfile::TempDir;

use crate::models::session::SessionType;

use super::RelayContext;

/// The tempdirs backing [`context_for`]'s [`RelayContext`] must outlive the
/// test — a dropped [`TempDir`] deletes its directory — so this bundles them
/// with the context and the cwd `RelayContext::check` should be called with.
pub(crate) struct RelayFixture {
    pub context: RelayContext,
    pub cwd: PathBuf,
    _scratch_root: TempDir,
    _project: TempDir,
}

/// Build a `RelayContext` for `session_type` that passes every guard in
/// [`RelayContext::check`] up to the verdict-matrix check: a scratch
/// directory that passes `validate_session_dir`, a `stage_id` of
/// `"stage-a"`, and a checkout boundary — `worktree_path` for
/// [`SessionType::Stage`], `work_dir` (a project root's `.loom/work`)
/// otherwise — that contains the returned cwd.
pub(crate) fn context_for(session_type: SessionType) -> RelayFixture {
    let scratch_root = TempDir::new().expect("create scratch root tempdir");
    let scratch = scratch_root.path().join("session-1");
    fs::create_dir(&scratch).expect("create scratch session dir");
    fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700))
        .expect("chmod scratch session dir");

    let project = TempDir::new().expect("create project tempdir");
    let work_dir = project.path().join(".loom").join("work");
    fs::create_dir_all(&work_dir).expect("create .loom/work");

    let (worktree_path, cwd) = if session_type == SessionType::Stage {
        let worktree = project.path().join("worktree");
        fs::create_dir_all(&worktree).expect("create stage worktree dir");
        (Some(worktree.clone()), worktree)
    } else {
        (None, project.path().to_path_buf())
    };

    let context = RelayContext {
        session_id: "session-1".to_string(),
        scratch_dir: scratch,
        stage_id: Some("stage-a".to_string()),
        session_type: Some(session_type.to_string()),
        worktree_path,
        work_dir: Some(work_dir),
    };

    RelayFixture {
        context,
        cwd,
        _scratch_root: scratch_root,
        _project: project,
    }
}
