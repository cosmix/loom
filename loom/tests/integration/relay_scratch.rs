//! Scratch-directory helpers shared by the relay end-to-end suites
//! (`relay_e2e.rs`, `relay_commit_e2e.rs`).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use loom::relay::{scratch_root, session_dir, Platform};
use tempfile::TempDir;

/// A fresh mode-0700 temp dir that does NOT resolve under `/tmp`, for the
/// `XDG_RUNTIME_DIR` (Linux) and `HOME` (macOS) the scratch root derives from:
/// `relay::scratch::scratch_root` refuses a root under `/tmp` (real sandboxes
/// mount `/tmp` world-writable, so a root there would let any sandboxed
/// process on the host plant tickets), and this suite's own `$TMPDIR` can
/// live under `/tmp` (`relay/scratch.rs`'s own tests hit the same
/// constraint). The bounded runner supplies a trusted sibling of TMPDIR;
/// direct Cargo invocations retain gitignored `target/` scratch space.
pub(super) fn tempdir_outside_tmp(prefix: &str) -> TempDir {
    let base = std::env::var_os("LOOM_TEST_ROOT")
        .map(PathBuf::from)
        .map(|root| root.join("trusted"))
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/relay-e2e-tmp"));
    fs::create_dir_all(&base).expect("create trusted relay fixture root");
    let dir = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(&base)
        .expect("create tempdir outside /tmp");
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).expect("chmod tempdir");
    dir
}

/// `<scratch root>/<session_id>`, mode 0700: the path the session launcher
/// exports as `LOOM_SCRATCH_DIR` and the relay hook requires, both through
/// `relay::scratch_root` for this `XDG_RUNTIME_DIR` and `HOME` —
/// `<xdg>/loom/scratch` on Linux, `<home>/Library/Caches/loom/scratch` on
/// macOS.
pub(super) fn create_scratch_dir(xdg: &Path, home: &Path, session_id: &str) -> PathBuf {
    let platform = if cfg!(target_os = "macos") {
        Platform::Macos
    } else {
        Platform::Linux
    };
    // The caller's real uid, as production `scratch_root_from_env` takes it:
    // reading the owner off the tempdir would make the owned-by-me check
    // compare the directory with itself.
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    let root = scratch_root(platform, Some(xdg), home, uid).expect("derive the scratch root");
    let scratch = session_dir(&root, session_id).expect("derive the session scratch dir");
    fs::create_dir_all(&scratch).expect("create scratch dir");
    fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700)).expect("chmod scratch dir");
    scratch
}
