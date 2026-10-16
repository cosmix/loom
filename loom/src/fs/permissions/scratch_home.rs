//! Test fixture: `HOME` pinned to a scratch directory.

use std::ffi::OsString;
use std::path::Path;
use tempfile::TempDir;

/// Points `HOME` at a scratch directory for its lifetime: creating a
/// worktree registers trust in `~/.claude.json`. Restores the previous value
/// on drop; tests that hold one must be `#[serial]`.
pub(crate) struct ScratchHome {
    _dir: TempDir,
    original: Option<OsString>,
}

impl ScratchHome {
    pub(crate) fn set() -> Self {
        let dir = TempDir::new().unwrap();
        let original = std::env::var_os("HOME");
        std::env::set_var("HOME", dir.path());
        Self {
            _dir: dir,
            original,
        }
    }

    pub(crate) fn path(&self) -> &Path {
        self._dir.path()
    }
}

impl Drop for ScratchHome {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
    }
}
